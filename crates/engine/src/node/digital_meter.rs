//! A digital level meter — the dBFS counterpart to the analog [`VuMeter`](super::VuMeter).

use super::Node;
use crate::level::sample_to_dbfs;
use crate::param::Params;
use crate::port::{AudioFormat, DigitalFace, InputPort, OutputPort};
use crate::readout::{ReadoutDecl, ReadoutId};
use crate::signal::{BitDepth, Lane, SampleRate};

/// A digital level meter: an inline passthrough node on one digital channel that measures the
/// samples flowing through it and reports **peak** and **RMS** levels in **dBFS** — the digital-side
/// companion to the analog [`VuMeter`](super::VuMeter). Placing a `VuMeter` before an AD and a
/// `DigitalMeter` after it makes gain-staging *across the converter* visible: the same signal read as
/// dBu on the analog side and dBFS on the digital side.
///
/// Both readouts are measured **per block** (the block is the integration window, ~21 ms at
/// 48 kHz / 1024 analog samples): the peak is the block's largest `|sample|`, the RMS its
/// root-mean-square. Full scale (±1.0) is 0 dBFS, so a signal that reaches ±1.0 reads **0 dBFS
/// peak** — the honest hard-clip indicator (a converter can't represent more). Passthrough is exact
/// (a sample copy), so the meter is transparent.
///
/// One digital channel in, one out — DSP is mono, matching the converters.
pub struct DigitalMeter {
    /// Block peak `|sample|` (linear, ±1.0 = full scale), from the block just processed.
    peak: f32,
    /// Block RMS `sample` (linear), from the block just processed.
    rms: f32,
    readouts: [ReadoutDecl; 2],
    inputs: [InputPort; 1],
    outputs: [OutputPort; 1],
}

impl DigitalMeter {
    /// The block **peak** level, in dBFS (`20·log10(peak)`, 0 dBFS = full scale). Read back with
    /// `(node, DigitalMeter::PEAK_DBFS)`.
    pub const PEAK_DBFS: ReadoutId = ReadoutId(0);
    /// The block **RMS** level, in dBFS. Sits ~3 dB below the peak for a sine (the crest factor).
    pub const RMS_DBFS: ReadoutId = ReadoutId(1);

    /// Reading floor so digital silence (−∞ dBFS) reports a finite, off-scale value.
    const READING_FLOOR_DB: f32 = -120.0;

    /// A digital meter for a `rate` / `bits` mono stream.
    #[must_use]
    pub fn new(rate: SampleRate, bits: BitDepth) -> Self {
        let face = DigitalFace::new(AudioFormat::new(rate, bits, 1));
        Self {
            peak: 0.0,
            rms: 0.0,
            readouts: [
                ReadoutDecl {
                    id: Self::PEAK_DBFS,
                },
                ReadoutDecl { id: Self::RMS_DBFS },
            ],
            inputs: [face.into()],
            outputs: [face.into()],
        }
    }

    /// A normalized sample level as dBFS, floored so silence reads a finite off-scale value.
    fn floored_dbfs(level: f32) -> f32 {
        if level > 0.0 {
            sample_to_dbfs(level).max(Self::READING_FLOOR_DB)
        } else {
            Self::READING_FLOOR_DB
        }
    }
}

impl Node for DigitalMeter {
    fn inputs(&self) -> &[InputPort] {
        &self.inputs
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    fn readouts(&self) -> &[ReadoutDecl] {
        &self.readouts
    }

    fn process(&mut self, _params: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
        let src = inputs[0].sample().as_slice();
        let out = outputs[0].sample_mut().as_mut_slice();
        let mut peak = 0.0_f32;
        let mut sum_sq = 0.0_f64; // f64 accumulator (the summing-precision rule)
        for (o, &s) in out.iter_mut().zip(src) {
            *o = s; // exact passthrough
            let mag = s.abs();
            if mag > peak {
                peak = mag;
            }
            sum_sq += f64::from(s) * f64::from(s);
        }
        self.peak = peak;
        self.rms = if src.is_empty() {
            0.0
        } else {
            (sum_sq / src.len() as f64).sqrt() as f32
        };
    }

    fn read_readouts(&self, out: &mut [f32]) {
        out[0] = Self::floored_dbfs(self.peak);
        out[1] = Self::floored_dbfs(self.rms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::{ClockDomainId, Domain, SampleBuffer};
    use approx::assert_relative_eq;

    fn fs() -> SampleRate {
        SampleRate::new(48_000.0)
    }
    fn bits() -> BitDepth {
        BitDepth::new(16)
    }

    /// A digital sine of normalized amplitude `amp` at `freq_hz`, `len` samples at 48 kHz.
    fn digital_sine(freq_hz: f64, amp: f32, len: usize) -> Vec<f32> {
        let step = freq_hz / fs().as_hz();
        (0..len)
            .map(|n| amp * (2.0 * std::f32::consts::PI * (step * n as f64) as f32).sin())
            .collect()
    }

    fn run(meter: &mut DigitalMeter, samples: Vec<f32>) -> [f32; 2] {
        let n = samples.len();
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
        meter.process(&Params::EMPTY, &inp, &mut out);
        let mut r = [0.0_f32; 2];
        meter.read_readouts(&mut r);
        r
    }

    #[test]
    fn declares_mono_digital_in_and_out_with_two_readouts() {
        let m = DigitalMeter::new(fs(), bits());
        assert_eq!(m.inputs()[0].domain(), Domain::DigitalAudio);
        assert_eq!(m.outputs()[0].domain(), Domain::DigitalAudio);
        assert_eq!(m.inputs()[0].lane_count(), 1);
        assert_eq!(m.readouts().len(), 2);
    }

    #[test]
    fn passes_samples_through_unchanged() {
        let mut m = DigitalMeter::new(fs(), bits());
        let samples = vec![0.1, -0.2, 0.3, -0.4];
        let inp = [Lane::Sample(SampleBuffer::from_samples(
            samples.clone(),
            fs(),
            bits(),
            ClockDomainId::SINGLE,
        ))];
        let mut out = [Lane::Sample(SampleBuffer::zeros(
            4,
            fs(),
            bits(),
            ClockDomainId::SINGLE,
        ))];
        m.process(&Params::EMPTY, &inp, &mut out);
        assert_eq!(out[0].sample().as_slice(), samples.as_slice());
    }

    #[test]
    fn reads_dbfs_for_a_half_scale_sine() {
        // Hand calc: a 0.5-full-scale sine has peak 0.5 ⇒ 20·log10(0.5) = −6.02 dBFS, and RMS
        // 0.5/√2 = 0.35355 ⇒ 20·log10(0.35355) = −9.03 dBFS (3.01 dB below peak, the crest factor).
        // 100 Hz over 20 whole cycles (9600 samples at 48 kHz) so the RMS is exact and a sample
        // lands essentially on the peak.
        let mut m = DigitalMeter::new(fs(), bits());
        let r = run(&mut m, digital_sine(100.0, 0.5, 9_600));
        assert_relative_eq!(r[0], -6.0206, epsilon = 0.05); // peak dBFS
        assert_relative_eq!(r[1], -9.031, epsilon = 0.05); // rms dBFS
    }

    #[test]
    fn full_scale_reads_zero_dbfs_peak() {
        // A signal reaching ±1.0 is at the converter's ceiling: 0 dBFS peak (the honest clip point).
        let mut m = DigitalMeter::new(fs(), bits());
        let r = run(&mut m, vec![1.0, -1.0, 0.5, -0.5]);
        assert_relative_eq!(r[0], 0.0, epsilon = 1e-6);
    }

    #[test]
    fn silence_reads_the_floor() {
        let mut m = DigitalMeter::new(fs(), bits());
        let r = run(&mut m, vec![0.0; 64]);
        assert_eq!(r[0], DigitalMeter::READING_FLOOR_DB);
        assert_eq!(r[1], DigitalMeter::READING_FLOOR_DB);
    }
}
