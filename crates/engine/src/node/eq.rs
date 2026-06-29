//! A three-band equalizer: a low shelf, a parametric mid, and a high shelf in series.

use super::Node;
use crate::dsp::Biquad;
use crate::param::Params;
use crate::port::{AudioFormat, DigitalFace, InputPort, OutputPort};
use crate::signal::{AnalogRate, BitDepth, Lane, SampleRate};

/// One EQ band's settings: center/corner `freq_hz`, `q` (bandwidth for the mid peak, transition
/// steepness for a shelf — ≈ 0.707 is a flat shelf), and `gain_db` of boost (+) or cut (−).
///
/// A `gain_db` of 0 makes the band transparent, so a band you don't want is simply set flat.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EqBand {
    /// Center (mid) or corner (shelf) frequency in hertz.
    pub freq_hz: f64,
    /// Q — bandwidth for the peak, transition steepness for a shelf.
    pub q: f64,
    /// Boost (+) or cut (−) in decibels; 0 is transparent.
    pub gain_db: f64,
}

impl EqBand {
    /// A band at `freq_hz` with quality `q` and `gain_db` of boost/cut.
    #[must_use]
    pub fn new(freq_hz: f64, q: f64, gain_db: f64) -> Self {
        Self {
            freq_hz,
            q,
            gain_db,
        }
    }
}

/// A **three-band EQ** operating in the digital domain (between the modeled AD and DA): a low
/// **shelf**, a parametric mid **peak**, and a high **shelf**, applied in series to one digital
/// channel.
///
/// The bands are **static** — set at construction and baked into three [`Biquad`]s at
/// [`prepare`](Node::prepare), the expensive `cos`/`sin`/`powf` of coefficient design paid once.
/// (Live, smoothed EQ control is deferred to the real-time epic; safely de-zippering biquad
/// coefficients is its own problem.) The three filters run as a cascade, so their responses
/// multiply — an all-0-dB EQ is exactly transparent.
///
/// One digital channel in, one out — DSP is mono, matching the converters.
pub struct ThreeBandEq {
    rate: SampleRate,
    low: EqBand,
    mid: EqBand,
    high: EqBand,
    /// The three cascaded biquads (low shelf, mid peak, high shelf), built at
    /// [`prepare`](Node::prepare). `None` until prepared (a direct unit test that skips `prepare`
    /// passes through).
    filters: Option<[Biquad; 3]>,
    inputs: [InputPort; 1],
    outputs: [OutputPort; 1],
}

impl ThreeBandEq {
    /// A three-band EQ for a `rate` / `bits` digital stream: `low` as a shelf, `mid` as a peak,
    /// `high` as a shelf.
    #[must_use]
    pub fn new(rate: SampleRate, bits: BitDepth, low: EqBand, mid: EqBand, high: EqBand) -> Self {
        let face = DigitalFace::new(AudioFormat::new(rate, bits, 1));
        Self {
            rate,
            low,
            mid,
            high,
            filters: None,
            inputs: [face.into()],
            outputs: [face.into()],
        }
    }
}

impl Node for ThreeBandEq {
    fn inputs(&self) -> &[InputPort] {
        &self.inputs
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    fn prepare(&mut self, _rate: AnalogRate) {
        // A pure-digital node: coefficients are designed against our own `SampleRate`. The analog
        // rate the schedule runs at is irrelevant here — nothing in this node touches it.
        self.filters = Some([
            Biquad::low_shelf(self.rate, self.low.freq_hz, self.low.q, self.low.gain_db),
            Biquad::peaking(self.rate, self.mid.freq_hz, self.mid.q, self.mid.gain_db),
            Biquad::high_shelf(self.rate, self.high.freq_hz, self.high.q, self.high.gain_db),
        ]);
    }

    fn process(&mut self, _params: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
        let src = inputs[0].sample().as_slice();
        let out = outputs[0].sample_mut().as_mut_slice();
        match &mut self.filters {
            // Run each sample through the three-filter cascade in one pass (no scratch buffer).
            Some(filters) => {
                for (o, &s) in out.iter_mut().zip(src) {
                    let mut x = f64::from(s);
                    for f in filters.iter_mut() {
                        x = f.step(x);
                    }
                    *o = x as f32;
                }
            }
            // Unprepared (direct unit test only): pass through unchanged.
            None => {
                for (o, &s) in out.iter_mut().zip(src) {
                    *o = s;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::{AnalogRate, ClockDomainId, Domain, SampleBuffer};
    use approx::assert_relative_eq;

    fn fs() -> SampleRate {
        SampleRate::new(48_000.0)
    }
    fn bits() -> BitDepth {
        BitDepth::new(24)
    }
    /// 48 kHz as an `AnalogRate`, only so `tone_amplitude` runs its DFT at the digital period.
    fn fs_as_analog() -> AnalogRate {
        AnalogRate::new(48_000.0)
    }

    /// A digital sine of normalized amplitude `amp` at `freq_hz`, `len` samples at 48 kHz.
    fn digital_sine(freq_hz: f64, amp: f32, len: usize) -> Vec<f32> {
        let step = freq_hz / fs().as_hz();
        (0..len)
            .map(|n| amp * (2.0 * std::f32::consts::PI * (step * n as f64) as f32).sin())
            .collect()
    }

    /// Run a sine of `freq_hz` through `eq` and return the gain relative to the input amplitude
    /// (the ratio cancels the single-bin DFT's leakage, leaving the EQ's own gain).
    fn gain_at(eq: &mut ThreeBandEq, freq_hz: f64) -> f32 {
        use crate::test_util::tone_amplitude;
        let n = 8_192;
        let samples = digital_sine(freq_hz, 0.5, n);
        let in_amp = tone_amplitude(&samples[n / 2..], freq_hz, fs_as_analog());
        let inp = [Lane::Sample(SampleBuffer::from_samples(
            samples,
            fs(),
            bits(),
            ClockDomainId::SINGLE,
        ))];
        let mut out = [Lane::Sample(SampleBuffer::zeros(
            n,
            fs(),
            bits(),
            ClockDomainId::SINGLE,
        ))];
        eq.process(&Params::EMPTY, &inp, &mut out);
        let out_amp = tone_amplitude(
            &out[0].sample().as_slice()[n / 2..],
            freq_hz,
            fs_as_analog(),
        );
        out_amp / in_amp
    }

    fn eq(low: EqBand, mid: EqBand, high: EqBand) -> ThreeBandEq {
        let mut e = ThreeBandEq::new(fs(), bits(), low, mid, high);
        e.prepare(fs_as_analog());
        e
    }

    #[test]
    fn declares_mono_digital_in_and_out() {
        let e = ThreeBandEq::new(
            fs(),
            bits(),
            EqBand::new(100.0, 0.707, 0.0),
            EqBand::new(1_000.0, 1.0, 0.0),
            EqBand::new(8_000.0, 0.707, 0.0),
        );
        assert_eq!(e.inputs()[0].domain(), Domain::DigitalAudio);
        assert_eq!(e.outputs()[0].domain(), Domain::DigitalAudio);
        assert_eq!(e.inputs()[0].lane_count(), 1);
        assert_eq!(e.outputs()[0].lane_count(), 1);
    }

    #[test]
    fn all_flat_is_transparent() {
        let mut e = eq(
            EqBand::new(120.0, 0.707, 0.0),
            EqBand::new(1_000.0, 1.0, 0.0),
            EqBand::new(8_000.0, 0.707, 0.0),
        );
        for &freq in &[80.0, 1_000.0, 12_000.0] {
            assert_relative_eq!(gain_at(&mut e, freq), 1.0, epsilon = 1e-3);
        }
    }

    #[test]
    fn low_shelf_boosts_lows_only() {
        // +6 dB low shelf at 200 Hz; mid & high flat. A 50 Hz tone sees ≈ +6 dB (×1.995); a
        // 10 kHz tone is left ≈ unity.
        let mut e = eq(
            EqBand::new(200.0, 0.707, 6.0),
            EqBand::new(1_000.0, 1.0, 0.0),
            EqBand::new(8_000.0, 0.707, 0.0),
        );
        assert_relative_eq!(
            gain_at(&mut e, 50.0),
            10.0_f32.powf(6.0 / 20.0),
            epsilon = 0.05
        );
        assert!((gain_at(&mut e, 10_000.0) - 1.0).abs() < 0.05);
    }

    #[test]
    fn mid_peak_bumps_its_center() {
        // +9 dB mid peak at 1 kHz; shelves flat. At center ≈ ×10^(9/20) ≈ 2.818.
        let mut e = eq(
            EqBand::new(120.0, 0.707, 0.0),
            EqBand::new(1_000.0, 2.0, 9.0),
            EqBand::new(8_000.0, 0.707, 0.0),
        );
        assert_relative_eq!(
            gain_at(&mut e, 1_000.0),
            10.0_f32.powf(9.0 / 20.0),
            epsilon = 0.05
        );
    }
}
