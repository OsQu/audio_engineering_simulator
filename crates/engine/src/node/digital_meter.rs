//! A digital level meter — the dBFS counterpart to the analog [`VuMeter`](super::VuMeter).

use super::Node;
use crate::level::sample_to_dbfs;
use crate::param::Params;
use crate::port::{AudioFormat, DigitalFace, InputPort, OutputPort};
use crate::readout::{ReadoutDecl, ReadoutId};
use crate::signal::{BitDepth, Lane, SampleRate};

/// A digital level meter: an inline passthrough node on an N-lane digital stream that measures the
/// samples flowing through it and reports per-lane **peak** and **RMS** levels in **dBFS** — the
/// digital-side companion to the analog [`VuMeter`](super::VuMeter). Placing a `VuMeter` before an AD
/// and a `DigitalMeter` after it makes gain-staging *across the converter* visible: the same signal
/// read as dBu on the analog side and dBFS on the digital side.
///
/// All readouts are measured **per block** (the block is the integration window, ~21 ms at
/// 48 kHz / 1024 analog samples): the peak is the block's largest `|sample|`, the RMS its
/// root-mean-square. Full scale (±1.0) is 0 dBFS, so a signal that reaches ±1.0 reads **0 dBFS
/// peak** — the honest hard-clip indicator (a converter can't represent more). Passthrough is exact
/// (a sample copy), so the meter is transparent.
///
/// **One N-lane port in, one out** — a multichannel stream (a DAW's USB sends) is metered behind a
/// single connector, each lane measured independently: lane `k` reports Peak at [`ReadoutId`] `2k`
/// and RMS at `2k + 1` (see [`peak_dbfs`](Self::peak_dbfs) / [`rms_dbfs`](Self::rms_dbfs)).
/// `channels = 1` is the plain mono meter, matching the mono converters/DSP around it.
pub struct DigitalMeter {
    /// Per-lane block peak `|sample|` (linear, ±1.0 = full scale), from the block just processed.
    peak: Vec<f32>,
    /// Per-lane block RMS `sample` (linear), from the block just processed.
    rms: Vec<f32>,
    readouts: Vec<ReadoutDecl>,
    inputs: [InputPort; 1],
    outputs: [OutputPort; 1],
}

impl DigitalMeter {
    /// **Lane 0's** block **peak** level, in dBFS (`20·log10(peak)`, 0 dBFS = full scale) — lane
    /// `k`'s id comes from [`peak_dbfs`](Self::peak_dbfs). Read back with
    /// `(node, DigitalMeter::PEAK_DBFS)`.
    pub const PEAK_DBFS: ReadoutId = ReadoutId(0);
    /// **Lane 0's** block **RMS** level, in dBFS — lane `k`'s id comes from
    /// [`rms_dbfs`](Self::rms_dbfs). Sits ~3 dB below the peak for a sine (the crest factor).
    pub const RMS_DBFS: ReadoutId = ReadoutId(1);

    /// Reading floor so digital silence (−∞ dBFS) reports a finite, off-scale value.
    const READING_FLOOR_DB: f32 = -120.0;

    /// The [`ReadoutId`] of lane `lane`'s block-**peak** reading: `2·lane` (each lane declares a
    /// (Peak, RMS) pair, in lane order). Lane 0's is [`PEAK_DBFS`](Self::PEAK_DBFS).
    #[must_use]
    pub fn peak_dbfs(lane: usize) -> ReadoutId {
        ReadoutId((2 * lane) as u32)
    }

    /// The [`ReadoutId`] of lane `lane`'s block-**RMS** reading: `2·lane + 1`. Lane 0's is
    /// [`RMS_DBFS`](Self::RMS_DBFS).
    #[must_use]
    pub fn rms_dbfs(lane: usize) -> ReadoutId {
        ReadoutId((2 * lane + 1) as u32)
    }

    /// A digital meter for an N-`channels` `rate` / `bits` stream.
    ///
    /// # Panics
    /// Panics if `channels == 0` — a meter needs at least one lane. Checked at construction.
    #[must_use]
    pub fn new(rate: SampleRate, bits: BitDepth, channels: u16) -> Self {
        assert!(channels >= 1, "DigitalMeter needs at least one channel");

        let face = DigitalFace::new(AudioFormat::new(rate, bits, channels));
        Self {
            peak: vec![0.0; usize::from(channels)],
            rms: vec![0.0; usize::from(channels)],
            readouts: (0..usize::from(channels))
                .flat_map(|i| {
                    [
                        ReadoutDecl {
                            id: Self::peak_dbfs(i),
                        },
                        ReadoutDecl {
                            id: Self::rms_dbfs(i),
                        },
                    ]
                })
                .collect(),
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
        for (k, (inp, out)) in inputs.iter().zip(outputs.iter_mut()).enumerate() {
            let src = inp.sample().as_slice();
            let out = out.sample_mut().as_mut_slice();
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
            self.peak[k] = peak;
            self.rms[k] = if src.is_empty() {
                0.0
            } else {
                (sum_sq / src.len() as f64).sqrt() as f32
            };
        }
    }

    fn read_readouts(&self, out: &mut [f32]) {
        for lane in 0..self.peak.len() {
            out[Self::peak_dbfs(lane).0 as usize] = Self::floored_dbfs(self.peak[lane]);
            out[Self::rms_dbfs(lane).0 as usize] = Self::floored_dbfs(self.rms[lane]);
        }
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

    /// Drive `meter` with one buffer per lane and return all `2·lanes` readings (`(peak, rms)`
    /// pairs, in lane order). Each lane moves into its own input `Lane`; outputs are zeroed
    /// same-length lanes.
    fn run(meter: &mut DigitalMeter, lanes: Vec<Vec<f32>>) -> Vec<f32> {
        let channels = lanes.len();
        let n = lanes[0].len();
        let inp: Vec<Lane> = lanes
            .into_iter()
            .map(|samples| {
                Lane::Sample(SampleBuffer::from_samples(
                    samples,
                    fs(),
                    bits(),
                    ClockDomainId::SINGLE,
                ))
            })
            .collect();
        let mut out: Vec<Lane> = (0..channels)
            .map(|_| Lane::Sample(SampleBuffer::zeros(n, fs(), bits(), ClockDomainId::SINGLE)))
            .collect();

        meter.process(&Params::EMPTY, &inp, &mut out);
        let mut r = vec![0.0_f32; 2 * channels];
        meter.read_readouts(&mut r);
        r
    }

    #[test]
    fn declares_mono_digital_in_and_out_with_two_readouts() {
        let m = DigitalMeter::new(fs(), bits(), 1);
        assert_eq!(m.inputs()[0].domain(), Domain::DigitalAudio);
        assert_eq!(m.outputs()[0].domain(), Domain::DigitalAudio);
        assert_eq!(m.inputs()[0].lane_count(), 1);
        assert_eq!(m.readouts().len(), 2);
    }

    #[test]
    fn passes_samples_through_unchanged() {
        let mut m = DigitalMeter::new(fs(), bits(), 1);
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
        let mut m = DigitalMeter::new(fs(), bits(), 1);
        let r = run(&mut m, vec![digital_sine(100.0, 0.5, 9_600)]);
        assert_relative_eq!(r[0], -6.0206, epsilon = 0.05); // peak dBFS
        assert_relative_eq!(r[1], -9.031, epsilon = 0.05); // rms dBFS
    }

    #[test]
    fn full_scale_reads_zero_dbfs_peak() {
        // A signal reaching ±1.0 is at the converter's ceiling: 0 dBFS peak (the honest clip point).
        let mut m = DigitalMeter::new(fs(), bits(), 1);
        let r = run(&mut m, vec![vec![1.0, -1.0, 0.5, -0.5]]);
        assert_relative_eq!(r[0], 0.0, epsilon = 1e-6);
    }

    #[test]
    fn silence_reads_the_floor() {
        let mut m = DigitalMeter::new(fs(), bits(), 1);
        let r = run(&mut m, vec![vec![0.0; 64]]);
        assert_eq!(r[0], DigitalMeter::READING_FLOOR_DB);
        assert_eq!(r[1], DigitalMeter::READING_FLOOR_DB);
    }

    #[test]
    fn meters_each_lane_independently() {
        // Three lanes with **distinct** amplitudes, so a lane↔id mixup (or one lane's reading
        // overwriting another's) is falsifiable — identical lanes would pass through any shuffle.
        // Hand calc per lane (peak = 20·log10(amp); sine RMS = peak − 3.0103 dB, the crest factor):
        //   lane 0: amp 1.0  ⇒ peak  0       dBFS, RMS  −3.0103 dBFS
        //   lane 1: amp 0.5  ⇒ peak −6.0206  dBFS, RMS  −9.0309 dBFS
        //   lane 2: amp 0.25 ⇒ peak −12.0412 dBFS, RMS −15.0515 dBFS
        // 100 Hz over 20 whole cycles (9600 samples at 48 kHz): the RMS is exact and sample 120
        // lands exactly on the quarter-cycle peak.
        let mut m = DigitalMeter::new(fs(), bits(), 3);
        assert_eq!(m.inputs()[0].lane_count(), 3);
        assert_eq!(m.outputs()[0].lane_count(), 3);
        assert_eq!(m.readouts().len(), 6);

        let r = run(
            &mut m,
            vec![
                digital_sine(100.0, 1.0, 9_600),
                digital_sine(100.0, 0.5, 9_600),
                digital_sine(100.0, 0.25, 9_600),
            ],
        );
        assert_relative_eq!(r[0], 0.0, epsilon = 0.05); // lane 0 peak
        assert_relative_eq!(r[1], -3.0103, epsilon = 0.05); // lane 0 RMS
        assert_relative_eq!(r[2], -6.0206, epsilon = 0.05); // lane 1 peak
        assert_relative_eq!(r[3], -9.0309, epsilon = 0.05); // lane 1 RMS
        assert_relative_eq!(r[4], -12.0412, epsilon = 0.05); // lane 2 peak
        assert_relative_eq!(r[5], -15.0515, epsilon = 0.05); // lane 2 RMS
    }
}
