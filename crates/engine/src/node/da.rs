//! The DA converter: the digital → analog boundary.

use super::Node;
use crate::electrical::{Ohms, OutputZ};
use crate::fir::{Interpolator, kaiser_beta};
use crate::param::Params;
use crate::port::{AudioFormat, DigitalFace, InputPort, OutputPort};
use crate::signal::{AnalogRate, BitDepth, Lane, SampleRate, Volts};

/// Default reconstruction filter length — a steep kernel matching the AD's anti-alias filter.
const DEFAULT_RECON_TAPS: usize = 161;
/// Default reconstruction stopband attenuation target, shaping the Kaiser window.
const DEFAULT_STOPBAND_DB: f64 = 96.0;

/// A **DA converter**: it lifts a digital stream back to the oversampled analog rate, the modeled
/// boundary where dBFS samples become volts. The mirror of [`AdConverter`](super::AdConverter).
///
/// Two stages, in order:
/// 1. **Reconstruction interpolation** — a polyphase windowed-sinc FIR ([`Interpolator`]) upsamples
///    the digital stream to the analog rate, inserting `M − 1` zeros between samples and low-passing
///    at the pre-upsampling Nyquist so the spectral images of the zero-stuffed signal are rejected.
///    Its tap count is the "weak filter" knob ([`with_recon_taps`](Self::with_recon_taps)); a short
///    kernel leaves images in the band (audible imaging).
/// 2. **De-calibration** — the normalized samples (±1.0 = full scale) are scaled by the converter's
///    **reference voltage** (the peak volts at full scale), so full scale leaves as ±reference volts.
///    The same calibration the AD applies in reverse; a 13.80 V-peak reference puts −18 dBFS back at
///    +4 dBu. Interpolation is linear, so this scaling commutes with stage 1 — it's done in place on
///    the analog output after interpolation.
///
/// One digital channel in, single-ended analog out (converters are mono; a balanced line driver is
/// a separate node). There is no quantization or dither here — the DA reads samples
/// that are already on the bit grid and produces continuous volts.
pub struct DaConverter {
    rate: SampleRate,
    /// Reconstruction FIR length — the "weak filter" knob.
    recon_taps: usize,
    /// Peak volts at digital full scale (normalized → volts).
    reference: f64,
    /// The reconstruction interpolator, built at [`prepare`](Node::prepare) once the factor is known.
    interp: Option<Interpolator>,
    inputs: [InputPort; 1],
    outputs: [OutputPort; 1],
}

impl DaConverter {
    /// A DA reading `rate` / `bits` samples, emitting `reference` peak volts at full scale, with
    /// output impedance `z_out`. Uses a steep default reconstruction filter.
    ///
    /// # Panics
    /// Panics unless `reference` is finite and `> 0` (the calibration would be degenerate).
    #[must_use]
    pub fn new(rate: SampleRate, bits: BitDepth, reference: Volts, z_out: Ohms) -> Self {
        let reference = reference.get();
        assert!(
            reference.is_finite() && reference > 0.0,
            "DA reference voltage must be finite and > 0, got {reference}"
        );
        Self {
            rate,
            recon_taps: DEFAULT_RECON_TAPS,
            reference: f64::from(reference),
            interp: None,
            inputs: [DigitalFace::new(AudioFormat::new(rate, bits, 1)).into()],
            outputs: [OutputZ::new(z_out).into()],
        }
    }

    /// Override the reconstruction filter length — the demonstrable "weak filter" knob. A short
    /// kernel can't reject the upsampling images, so they leak through (audible imaging).
    #[must_use]
    pub fn with_recon_taps(mut self, taps: usize) -> Self {
        self.recon_taps = taps;
        self
    }

    /// The converter's sample rate.
    pub fn rate(&self) -> SampleRate {
        self.rate
    }
}

impl Node for DaConverter {
    fn inputs(&self) -> &[InputPort] {
        &self.inputs
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    fn prepare(&mut self, rate: AnalogRate) {
        // Interpolation factor M = analog rate / digital rate. The integer-divide and block-length
        // constraints were already validated when `compile` sized this DA's input lane.
        let m = (rate.as_hz() / self.rate.as_hz()).round().max(1.0) as usize;
        self.interp = Some(Interpolator::lowpass(
            self.recon_taps,
            m,
            kaiser_beta(DEFAULT_STOPBAND_DB),
        ));
    }

    fn group_delay_samples(&self) -> f64 {
        // The reconstruction interpolator's linear-phase group delay (in analog-rate samples); 0
        // until `prepare` builds it, which `compile` always does before this is read.
        self.interp.as_ref().map_or(0.0, Interpolator::group_delay)
    }

    fn process(&mut self, _params: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
        let src = inputs[0].sample().as_slice();
        let out = outputs[0].voltage_mut().as_mut_slice();

        // 1. Reconstruct: interpolate the normalized samples up to the analog rate (still normalized).
        if let Some(interp) = &mut self.interp {
            interp.process(src, out);
        }

        // 2. De-calibrate: full scale ±1.0 → ±reference volts. Linear, so it commutes with stage 1.
        let reference = self.reference;
        for v in out.iter_mut() {
            *v = (f64::from(*v) * reference) as f32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::dbu_to_volts;
    use crate::signal::{ClockDomainId, Domain, SampleBuffer, VoltageBuffer};
    use crate::test_util::tone_amplitude;
    use approx::assert_relative_eq;

    fn hi() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }
    fn lo() -> SampleRate {
        SampleRate::new(48_000.0)
    }
    /// 48 kHz as an `AnalogRate`, purely so a digital tone can be synthesized at the sample rate.
    fn lo_as_analog() -> AnalogRate {
        AnalogRate::new(48_000.0)
    }

    /// Run a DA directly over `samples` (digital) and return its analog output (8× longer).
    fn run(da: &mut DaConverter, samples: &[f32]) -> Vec<f32> {
        let inp = [Lane::Sample(SampleBuffer::from_samples(
            samples.to_vec(),
            lo(),
            BitDepth::new(24),
            ClockDomainId::SINGLE,
        ))];
        let mut out = [Lane::Voltage(VoltageBuffer::zeros(samples.len() * 8, hi()))];
        da.process(&Params::EMPTY, &inp, &mut out);
        out[0].voltage().as_slice().to_vec()
    }

    /// A digital sine of normalized amplitude `amp` at `freq`, `len` samples at the 48 kHz rate.
    fn digital_sine(freq_hz: f64, amp: f32, len: usize) -> Vec<f32> {
        let step = freq_hz / lo().as_hz();
        (0..len)
            .map(|n| amp * (2.0 * std::f32::consts::PI * (step * n as f64) as f32).sin())
            .collect()
    }

    #[test]
    fn declares_mono_digital_in_single_ended_analog_out() {
        let da = DaConverter::new(lo(), BitDepth::new(24), Volts::new(13.80), Ohms::new(150.0));
        let inp = da.inputs()[0];
        assert_eq!(inp.domain(), Domain::DigitalAudio);
        assert_eq!(inp.lane_count(), 1); // mono
        assert_eq!(inp.digital().unwrap().format().rate(), lo());
        assert_eq!(da.outputs()[0].domain(), Domain::Analog);
        assert_eq!(da.outputs()[0].lane_count(), 1); // single-ended
    }

    #[test]
    fn half_full_scale_reconstructs_to_half_reference() {
        // 0.5 normalized into a 10 V-peak reference ⇒ 5 V analog, once the filter settles.
        let mut da = DaConverter::new(lo(), BitDepth::new(24), Volts::new(10.0), Ohms::new(150.0));
        da.prepare(hi());
        let out = run(&mut da, &[0.5_f32; 960]);
        // Past settling (group delay ~161/2 hi-rate samples ≈ 80), the tail sits at 5 V.
        assert!(
            out[800..].iter().all(|&v| (v - 5.0).abs() < 1e-2),
            "0.5 full-scale should reconstruct to half the reference voltage"
        );
    }

    #[test]
    fn minus_18_dbfs_reconstructs_to_plus_4_dbu() {
        // The AD calibration in reverse: −18 dBFS (0.12589 normalized peak) into a 13.80 V-peak
        // reference comes back out at +4 dBu (1.737 V peak). 0.12589 = 10^(−18/20).
        let mut da = DaConverter::new(lo(), BitDepth::new(24), Volts::new(13.80), Ohms::new(150.0));
        da.prepare(hi());
        let norm_peak = 10.0_f32.powf(-18.0 / 20.0); // −18 dBFS as a normalized peak
        let samples = digital_sine(1_000.0, norm_peak, 960);
        let out = run(&mut da, &samples);
        let amp = tone_amplitude(&out[800..], 1_000.0, hi());
        let expected_peak = dbu_to_volts(4.0).get() * std::f32::consts::SQRT_2; // ≈ 1.737 V
        assert_relative_eq!(amp, expected_peak, epsilon = 0.02);
    }

    #[test]
    fn passband_tone_reconstructs_at_unity() {
        // A −6 dBFS, 1 kHz digital tone into a 1 V reference comes out as a 0.5 V-peak analog tone.
        let mut da = DaConverter::new(lo(), BitDepth::new(24), Volts::new(1.0), Ohms::new(150.0));
        da.prepare(hi());
        let samples = digital_sine(1_000.0, 0.5, 960);
        let out = run(&mut da, &samples);
        let amp = tone_amplitude(&out[800..], 1_000.0, hi());
        assert_relative_eq!(amp, 0.5, epsilon = 0.01);
        // Sanity: the synthesized tone period divides the buffer, so `lo_as_analog` reads the same
        // 1 kHz when measured on the digital samples directly.
        let dig_amp = tone_amplitude(&samples[100..], 1_000.0, lo_as_analog());
        assert_relative_eq!(dig_amp, 0.5, epsilon = 0.01);
    }
}
