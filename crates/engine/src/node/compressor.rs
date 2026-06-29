//! A feed-forward dynamics compressor.

use super::Node;
use crate::dsp::PeakEnvelope;
use crate::param::Params;
use crate::port::{AudioFormat, DigitalFace, InputPort, OutputPort};
use crate::signal::{AnalogRate, BitDepth, Lane, SampleRate};

/// Envelope floor before taking its logarithm — about −240 dBFS, so digital silence maps to a
/// huge-negative level (well below any threshold ⇒ no gain reduction) without `log10(0) = −∞`.
const LEVEL_FLOOR: f64 = 1e-12;

/// A **feed-forward compressor**: a peak detector watches the level, a gain computer turns level
/// over a threshold into gain reduction, and a manual makeup gain restores level afterward.
///
/// The classic downward compressor, in the digital domain (between the modeled AD and DA). Stages,
/// per sample:
/// 1. **Detect** — a [`PeakEnvelope`] follows the rectified input with `attack` / `release` time
///    constants (baked at [`prepare`](Node::prepare)).
/// 2. **Compute gain** (in **dB**, where threshold / ratio / knee are natural): the envelope level
///    in dBFS, the amount it sits over the threshold scaled by `1/ratio − 1`, smoothed through an
///    optional soft **knee** of `knee_db` width (hard knee when 0). The dB result becomes a linear
///    gain.
/// 3. **Apply + makeup** — multiply the sample by that gain and by a fixed **makeup** gain. Makeup
///    is **manual** on purpose: the level drop from compression is visible, and matching it back is
///    the gain-staging lesson, not something hidden by auto-makeup.
///
/// **Feed-forward, no lookahead** — the gain is computed from the current envelope and applied to
/// the current sample (no delay buffer). One digital channel in, one out (DSP is mono). The realism
/// budget stays on the volts-and-converters layer; this transform stays legible.
pub struct Compressor {
    threshold_db: f64,
    /// `1/ratio − 1` ≤ 0 — the slope of dB gain reduction per dB over threshold (0 ⇒ no compression).
    slope: f64,
    knee_db: f64,
    attack_ms: f64,
    release_ms: f64,
    /// Linear makeup gain (`10^(makeup_db/20)`), precomputed.
    makeup_lin: f64,
    /// The peak detector, built at [`prepare`](Node::prepare). `None` ⇒ unprepared (direct unit
    /// test): the detector is instantaneous (envelope = |input|).
    envelope: Option<PeakEnvelope>,
    inputs: [InputPort; 1],
    outputs: [OutputPort; 1],
}

impl Compressor {
    /// A compressor on a `rate` / `bits` digital stream: `threshold_db` (dBFS) above which it acts,
    /// compression `ratio` (≥ 1; 1 = none), `attack_ms` / `release_ms` detector time constants. Hard
    /// knee and unity makeup by default — see [`with_knee`](Self::with_knee) /
    /// [`with_makeup`](Self::with_makeup).
    ///
    /// # Panics
    /// Panics unless `ratio ≥ 1` and the times are finite and `≥ 0` — setup bugs, caught at
    /// construction, never on the hot path.
    #[must_use]
    pub fn new(
        rate: SampleRate,
        bits: BitDepth,
        threshold_db: f64,
        ratio: f64,
        attack_ms: f64,
        release_ms: f64,
    ) -> Self {
        assert!(
            ratio.is_finite() && ratio >= 1.0,
            "Compressor ratio must be finite and ≥ 1, got {ratio}"
        );
        assert!(
            attack_ms.is_finite()
                && attack_ms >= 0.0
                && release_ms.is_finite()
                && release_ms >= 0.0,
            "Compressor attack/release must be finite and ≥ 0, got {attack_ms}/{release_ms}"
        );
        let face = DigitalFace::new(AudioFormat::new(rate, bits, 1));
        Self {
            threshold_db,
            slope: 1.0 / ratio - 1.0,
            knee_db: 0.0,
            attack_ms,
            release_ms,
            makeup_lin: 1.0,
            envelope: None,
            inputs: [face.into()],
            outputs: [face.into()],
        }
    }

    /// Add a soft knee of `knee_db` width (in dB, centered on the threshold): compression eases in
    /// over that range instead of cornering hard. Builder style. `knee_db ≤ 0` keeps a hard knee.
    #[must_use]
    pub fn with_knee(mut self, knee_db: f64) -> Self {
        self.knee_db = knee_db.max(0.0);
        self
    }

    /// Add `makeup_db` of fixed makeup gain applied after compression. Builder style.
    #[must_use]
    pub fn with_makeup(mut self, makeup_db: f64) -> Self {
        self.makeup_lin = 10.0_f64.powf(makeup_db / 20.0);
        self
    }

    /// The linear gain (≤ 1) to apply for an envelope level of `env`, from the dB gain computer.
    ///
    /// `over` is how far the level sits above the threshold (dB). Below the knee there's no
    /// reduction; above it the full `slope · over`; within a soft knee the two join with the
    /// standard quadratic interpolation. The dB reduction is converted back to a linear multiplier.
    #[inline]
    fn gain_for(&self, env: f64) -> f64 {
        let level_db = 20.0 * env.max(LEVEL_FLOOR).log10();
        let over = level_db - self.threshold_db;
        let reduction_db = if self.knee_db > 0.0 {
            if 2.0 * over <= -self.knee_db {
                0.0
            } else if 2.0 * over >= self.knee_db {
                self.slope * over
            } else {
                // Quadratic knee: ramps the slope in over the knee width (RBJ-style soft knee).
                let t = over + self.knee_db / 2.0;
                self.slope * t * t / (2.0 * self.knee_db)
            }
        } else if over > 0.0 {
            self.slope * over
        } else {
            0.0
        };
        10.0_f64.powf(reduction_db / 20.0)
    }
}

impl Node for Compressor {
    fn inputs(&self) -> &[InputPort] {
        &self.inputs
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    fn prepare(&mut self, _rate: AnalogRate) {
        // A pure-digital node: the detector's time constants are in the digital sample rate, which
        // the face carries. The analog rate the schedule runs at is irrelevant here.
        let rate = self.inputs[0]
            .digital()
            .expect("compressor input is digital")
            .format()
            .rate();
        self.envelope = Some(PeakEnvelope::new(rate, self.attack_ms, self.release_ms));
    }

    fn process(&mut self, _params: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
        let src = inputs[0].sample().as_slice();
        let out = outputs[0].sample_mut().as_mut_slice();
        let makeup = self.makeup_lin;
        match self.envelope.take() {
            Some(mut env) => {
                for (o, &s) in out.iter_mut().zip(src) {
                    let x = f64::from(s);
                    let level = env.step(x);
                    *o = (x * self.gain_for(level) * makeup) as f32;
                }
                self.envelope = Some(env);
            }
            // Unprepared (direct unit test): instantaneous detector, envelope = |sample|.
            None => {
                for (o, &s) in out.iter_mut().zip(src) {
                    let x = f64::from(s);
                    *o = (x * self.gain_for(x.abs()) * makeup) as f32;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::sample_to_dbfs;
    use crate::signal::{ClockDomainId, Domain, SampleBuffer};
    use approx::assert_relative_eq;

    fn fs() -> SampleRate {
        SampleRate::new(48_000.0)
    }
    fn bits() -> BitDepth {
        BitDepth::new(24)
    }

    /// Feed `len` samples of constant `level` (normalized) through `comp` and return the settled
    /// output (the last sample), once the envelope has reached the steady state.
    fn settled_output(comp: &mut Compressor, level: f32, len: usize) -> f32 {
        let inp = [Lane::Sample(SampleBuffer::from_samples(
            vec![level; len],
            fs(),
            bits(),
            ClockDomainId(0),
        ))];
        let mut out = [Lane::Sample(SampleBuffer::zeros(
            len,
            fs(),
            bits(),
            ClockDomainId(0),
        ))];
        comp.process(&Params::EMPTY, &inp, &mut out);
        *out[0].sample().as_slice().last().unwrap()
    }

    /// A compressor with a fast detector so a constant input settles quickly within the test block.
    fn comp(threshold_db: f64, ratio: f64) -> Compressor {
        let mut c = Compressor::new(fs(), bits(), threshold_db, ratio, 1.0, 1.0);
        c.prepare(AnalogRate::new(48_000.0));
        c
    }

    #[test]
    fn declares_mono_digital_in_and_out() {
        let c = Compressor::new(fs(), bits(), -10.0, 4.0, 5.0, 50.0);
        assert_eq!(c.inputs()[0].domain(), Domain::DigitalAudio);
        assert_eq!(c.outputs()[0].domain(), Domain::DigitalAudio);
        assert_eq!(c.inputs()[0].lane_count(), 1);
        assert_eq!(c.outputs()[0].lane_count(), 1);
    }

    #[test]
    fn below_threshold_passes_unchanged() {
        // −20 dBFS (0.1 normalized) under a −10 dBFS threshold ⇒ no reduction, unity makeup.
        let mut c = comp(-10.0, 4.0);
        let out = settled_output(&mut c, 0.1, 4_096);
        assert_relative_eq!(out, 0.1, epsilon = 1e-4);
    }

    #[test]
    fn above_threshold_follows_the_static_curve() {
        // Hand calc: ratio 4:1, threshold −10 dBFS, input −2 dBFS.
        //   over = −2 − (−10) = 8 dB;  reduction = (1/4 − 1)·8 = −6 dB ⇒ output −8 dBFS.
        let level = 10.0_f32.powf(-2.0 / 20.0); // −2 dBFS as a normalized DC level
        let mut c = comp(-10.0, 4.0);
        let out = settled_output(&mut c, level, 4_096);
        assert_relative_eq!(sample_to_dbfs(out), -8.0, epsilon = 0.05);
    }

    #[test]
    fn ratio_one_is_transparent() {
        // A 1:1 "compressor" has zero slope ⇒ no gain change anywhere, even above threshold.
        let level = 10.0_f32.powf(-2.0 / 20.0);
        let mut c = comp(-10.0, 1.0);
        let out = settled_output(&mut c, level, 4_096);
        assert_relative_eq!(out, level, epsilon = 1e-4);
    }

    #[test]
    fn makeup_gain_restores_level() {
        // Same −2 dBFS in, 4:1 @ −10 ⇒ −6 dB reduction; +6 dB makeup brings it back to −2 dBFS.
        let level = 10.0_f32.powf(-2.0 / 20.0);
        let mut c = Compressor::new(fs(), bits(), -10.0, 4.0, 1.0, 1.0).with_makeup(6.0);
        c.prepare(AnalogRate::new(48_000.0));
        let out = settled_output(&mut c, level, 4_096);
        assert_relative_eq!(sample_to_dbfs(out), -2.0, epsilon = 0.05);
    }

    #[test]
    fn soft_knee_compresses_at_the_threshold() {
        // Exactly at the threshold, a hard knee applies no reduction (over = 0), but a soft knee is
        // already compressing (it eases in over ±knee/2). So the soft-knee output is the quieter.
        let level = 10.0_f32.powf(-10.0 / 20.0); // −10 dBFS = the threshold
        let mut hard = comp(-10.0, 4.0);
        let mut soft = Compressor::new(fs(), bits(), -10.0, 4.0, 1.0, 1.0).with_knee(12.0);
        soft.prepare(AnalogRate::new(48_000.0));
        let out_hard = settled_output(&mut hard, level, 4_096);
        let out_soft = settled_output(&mut soft, level, 4_096);
        assert_relative_eq!(out_hard, level, epsilon = 1e-4); // hard knee: untouched at threshold
        assert!(
            out_soft < out_hard,
            "soft knee should already be compressing at the threshold: soft {out_soft} vs hard {out_hard}"
        );
    }

    #[test]
    #[should_panic(expected = "ratio must be finite and ≥ 1")]
    fn rejects_ratio_below_one() {
        let _ = Compressor::new(fs(), bits(), -10.0, 0.5, 5.0, 50.0);
    }
}
