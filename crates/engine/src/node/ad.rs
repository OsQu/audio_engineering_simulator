//! The AD converter: the analog → digital boundary.

use super::Node;
use crate::electrical::{InputZ, Ohms};
use crate::fir::{Decimator, kaiser_beta};
use crate::param::Params;
use crate::port::{AudioFormat, DigitalFace, InputPort, OutputPort};
use crate::rng::Rng;
use crate::signal::{AnalogRate, BitDepth, Lane, SampleRate, Volts};

/// Default anti-alias filter length — a steep kernel that rejects well past the decimated Nyquist.
const DEFAULT_AA_TAPS: usize = 161;
/// Default anti-alias stopband attenuation target, shaping the Kaiser window.
const DEFAULT_STOPBAND_DB: f64 = 96.0;

/// An **AD converter**: it samples the analog voltage at its own digital rate, the modeled
/// boundary where volts become dBFS samples.
///
/// Three stages, in order:
/// 1. **Anti-alias decimation** — a polyphase windowed-sinc FIR ([`Decimator`]) band-limits to the
///    decimated Nyquist and drops the oversampled analog stream to the converter's sample rate, so
///    nothing folds back. Its tap count is the demonstrable "weak filter" knob
///    ([`with_aa_taps`](Self::with_aa_taps)).
/// 2. **Calibration** — the decimated volts are normalized by the converter's **reference voltage**
///    (the peak volts at digital full scale), so a signal at the reference reads 0 dBFS. This is the
///    volts↔dBFS calibration the rest of the engine reads via
///    [`sample_to_dbfs`](crate::sample_to_dbfs); e.g. a 13.80 V-peak reference puts +4 dBu at
///    −18 dBFS.
/// 3. **Quantization** — round-to-nearest onto the bit-depth grid (`Δ = 1 / 2^(bits−1)`,
///    **mid-tread** so silence stays silent), hard-clamped at full scale (a digital over clips),
///    with **non-subtractive TPDF dither** (±1 LSB, from the seeded per-node RNG) decorrelating the
///    error into a flat noise floor.
///
/// Single-ended analog input, one digital channel out (Story 1.6 keeps converters mono; a balanced
/// front-end is a separate `BalancedReceiver` / preamp node). It **opens a clock domain**: every
/// sample it produces is stamped with the converter's rate (the emergent multi-domain model is
/// Epic 5).
pub struct AdConverter {
    rate: SampleRate,
    /// Anti-alias FIR length — the "weak filter" knob.
    aa_taps: usize,
    /// `1 / reference`, precomputed (volts → normalized).
    inv_ref: f64,
    /// Quantization step on the normalized scale, `1 / 2^(bits−1)`.
    delta: f64,
    /// The anti-alias decimator, built at [`prepare`](Node::prepare) once the factor is known.
    decimator: Option<Decimator>,
    /// The seeded dither stream, installed at [`seed`](Node::seed). `None` ⇒ undithered.
    dither: Option<Rng>,
    inputs: [InputPort; 1],
    outputs: [OutputPort; 1],
}

impl AdConverter {
    /// An AD sampling at `rate` / `bits`, with `reference` peak volts at full scale, presenting
    /// input impedance `z_in`. Uses a steep default anti-alias filter.
    ///
    /// # Panics
    /// Panics unless `reference` is finite and `> 0` (the calibration would be degenerate).
    #[must_use]
    pub fn new(rate: SampleRate, bits: BitDepth, reference: Volts, z_in: Ohms) -> Self {
        let reference = reference.get();
        assert!(
            reference.is_finite() && reference > 0.0,
            "AD reference voltage must be finite and > 0, got {reference}"
        );
        Self {
            rate,
            aa_taps: DEFAULT_AA_TAPS,
            inv_ref: 1.0 / f64::from(reference),
            delta: bits.step(1.0),
            decimator: None,
            dither: None,
            inputs: [InputZ::new(z_in).into()],
            outputs: [DigitalFace::new(AudioFormat::new(rate, bits, 1)).into()],
        }
    }

    /// Override the anti-alias filter length — the demonstrable "weak filter" knob. A short kernel
    /// can't reject content above the decimated Nyquist, so it folds back (audible aliasing).
    #[must_use]
    pub fn with_aa_taps(mut self, taps: usize) -> Self {
        self.aa_taps = taps;
        self
    }

    /// The converter's sample rate.
    pub fn rate(&self) -> SampleRate {
        self.rate
    }
}

impl Node for AdConverter {
    fn inputs(&self) -> &[InputPort] {
        &self.inputs
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    fn prepare(&mut self, rate: AnalogRate) {
        // Decimation factor M = analog rate / digital rate. The integer-divide and block-length
        // constraints were already validated when `compile` sized this AD's output lane.
        let m = (rate.as_hz() / self.rate.as_hz()).round().max(1.0) as usize;
        self.decimator = Some(Decimator::lowpass(
            self.aa_taps,
            m,
            kaiser_beta(DEFAULT_STOPBAND_DB),
        ));
    }

    fn seed(&mut self, rng: Rng) {
        self.dither = Some(rng);
    }

    fn process(&mut self, _params: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
        let src = inputs[0].voltage().as_slice();
        let out = outputs[0].sample_mut().as_mut_slice();

        // 1. Anti-alias decimate the analog volts straight into the output slots (still volts).
        if let Some(dec) = &mut self.decimator {
            dec.process(src, out);
        }

        // 2. + 3. Normalize by the reference, clamp at full scale, dither, and quantize — in place.
        let (inv_ref, delta) = (self.inv_ref, self.delta);
        match &mut self.dither {
            Some(rng) => {
                for s in out.iter_mut() {
                    let norm = (f64::from(*s) * inv_ref).clamp(-1.0, 1.0);
                    // TPDF (±1 LSB): the sum of two ±½-LSB uniform draws.
                    let d = (f64::from(rng.next_f32_unit()) - 0.5 + f64::from(rng.next_f32_unit())
                        - 0.5)
                        * delta;
                    *s = (((norm + d) / delta).round() * delta).clamp(-1.0, 1.0) as f32;
                }
            }
            None => {
                for s in out.iter_mut() {
                    let norm = (f64::from(*s) * inv_ref).clamp(-1.0, 1.0);
                    *s = ((norm / delta).round() * delta).clamp(-1.0, 1.0) as f32;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::{dbu_to_volts, sample_to_dbfs};
    use crate::signal::{ClockDomainId, Domain, SampleBuffer, VoltageBuffer};
    use crate::test_util::{sine, tone_amplitude};
    use approx::assert_relative_eq;

    fn hi() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }
    fn lo() -> SampleRate {
        SampleRate::new(48_000.0)
    }
    /// 48 kHz expressed as an `AnalogRate`, purely so [`tone_amplitude`]'s DFT runs at the digital
    /// rate (it only needs the sample period).
    fn lo_as_analog() -> AnalogRate {
        AnalogRate::new(48_000.0)
    }

    /// Run an AD directly over `input_volts` (length a multiple of 8) and return its samples.
    fn run(ad: &mut AdConverter, input_volts: &[f32], bits: BitDepth) -> Vec<f32> {
        let inp = [Lane::Voltage(VoltageBuffer::from_volts(
            input_volts.to_vec(),
            hi(),
        ))];
        let mut out = [Lane::Sample(SampleBuffer::zeros(
            input_volts.len() / 8,
            lo(),
            bits,
            ClockDomainId(0),
        ))];
        ad.process(&Params::EMPTY, &inp, &mut out);
        out[0].sample().as_slice().to_vec()
    }

    #[test]
    fn declares_single_ended_analog_in_mono_digital_out() {
        let ad = AdConverter::new(lo(), BitDepth::new(24), Volts::new(13.80), Ohms::new(1e6));
        assert_eq!(ad.inputs()[0].domain(), Domain::Analog);
        assert_eq!(ad.inputs()[0].lane_count(), 1); // single-ended
        let out = ad.outputs()[0];
        assert_eq!(out.domain(), Domain::DigitalAudio);
        assert_eq!(out.lane_count(), 1); // mono
        assert_eq!(out.digital().unwrap().format().rate(), lo());
    }

    #[test]
    fn plus_4_dbu_calibrates_to_minus_18_dbfs() {
        // Reference 13.80 V peak puts +4 dBu (1.737 V peak) at −18 dBFS.
        let bits = BitDepth::new(24);
        let mut ad = AdConverter::new(lo(), bits, Volts::new(13.80), Ohms::new(1e6));
        ad.prepare(hi());
        ad.seed(Rng::from_seed(1));

        let peak = dbu_to_volts(4.0).get() * std::f32::consts::SQRT_2; // +4 dBu peak ≈ 1.737 V
        // 7680 analog samples ⇒ 960 digital (whole cycles of 1 kHz at 48 kHz: 48 samples/cycle).
        let input = sine(1_000.0, Volts::new(peak), 7_680, hi());
        let out = run(&mut ad, input.as_slice(), bits);

        // Read the tone's peak (drop the first half as the filter transient) and convert to dBFS.
        let amp = tone_amplitude(&out[480..], 1_000.0, lo_as_analog());
        assert_relative_eq!(sample_to_dbfs(amp), -18.0, epsilon = 0.1);
    }

    #[test]
    fn dc_at_half_reference_is_half_full_scale() {
        // DC passes the anti-alias filter at unity; 5 V into a 10 V-peak reference ⇒ 0.5 normalized.
        let bits = BitDepth::new(24);
        let mut ad = AdConverter::new(lo(), bits, Volts::new(10.0), Ohms::new(1e6));
        ad.prepare(hi());
        ad.seed(Rng::from_seed(7));
        let out = run(&mut ad, &[5.0_f32; 7_680], bits);
        // Past settling, the steady tail sits at half scale.
        assert!(
            out[480..].iter().all(|&s| (s - 0.5).abs() < 1e-3),
            "DC at half the reference should read 0.5 full-scale"
        );
    }

    #[test]
    fn quantizes_to_the_bit_grid() {
        // 4-bit, undithered (no seed): Δ = 1/2^3 = 0.125. A 0.3-of-reference DC normalizes to 0.3,
        // which rounds to the nearest grid step: round(0.3 / 0.125) = 2 ⇒ 0.25.
        let bits = BitDepth::new(4);
        let mut ad = AdConverter::new(lo(), bits, Volts::new(1.0), Ohms::new(1e6));
        ad.prepare(hi()); // no seed ⇒ no dither ⇒ deterministic grid
        let out = run(&mut ad, &[0.3_f32; 7_680], bits);
        assert!(
            out[480..].iter().all(|&s| (s - 0.25).abs() < 1e-6),
            "0.3 should quantize to the 0.25 grid step at 4 bits"
        );
    }
}
