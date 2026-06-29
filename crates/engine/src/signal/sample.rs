//! A block of digital-audio samples.

use super::{BitDepth, SampleRate};

/// Identifies the clock domain — the oscillator — that produced a digital stream.
///
/// With a single internal converter clock this is a trivial identity. It is the *identity* of a
/// clock, not its rate (the rate rides in [`SampleRate`]), so multiple domains drifting against
/// each other — an async-boundary FIFO that slips, sample-rate conversion — can grow in later
/// without reshaping the buffer.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClockDomainId(pub u32);

impl ClockDomainId {
    /// The one clock domain every digital stream currently belongs to.
    ///
    /// Today there is exactly one domain: every converter runs off the single analog reference rate
    /// at an exact integer ratio, so nothing can drift and the domain is *assigned*, not *derived*.
    /// This constant is that assignment's single owner — the one place the "one domain" assumption
    /// lives. When devices declare a **clock source** (internal crystal / recovered-from-input /
    /// external word clock) and clocking is resolved as a distribution side-graph at compile, the
    /// domain a stream belongs to becomes *resolved from the producing device's source* rather than
    /// fixed here, and independently-clocked domains can drift and slip at their async boundaries.
    pub const SINGLE: ClockDomainId = ClockDomainId(0);
}

/// A block of single-channel digital-audio samples: **linear, normalized** so ±1.0 is full scale.
///
/// The digital-domain peer of [`VoltageBuffer`](super::VoltageBuffer). Linear only — dBFS is a
/// *measurement* produced by conversion helpers, never a storage format, exactly as volts are
/// stored linear and dBu is derived. One **channel** per buffer (a multichannel digital
/// port owns several, as a balanced analog port owns two conductors). It carries the rate, bit
/// depth, and clock domain stamped on by the AD that produced it. All allocation happens at
/// construction; the hot path mutates an existing buffer via [`as_mut_slice`](Self::as_mut_slice)
/// and never allocates.
#[derive(Debug, Clone, PartialEq)]
pub struct SampleBuffer {
    /// Linear normalized samples (±1.0 = full scale), one per sample; length is the block length.
    values: Vec<f32>,
    rate: SampleRate,
    bits: BitDepth,
    clock: ClockDomainId,
}

impl SampleBuffer {
    /// A zero-filled (digital silence) buffer of `len` samples at `rate`/`bits`/`clock`.
    #[must_use]
    pub fn zeros(len: usize, rate: SampleRate, bits: BitDepth, clock: ClockDomainId) -> Self {
        Self {
            values: vec![0.0; len],
            rate,
            bits,
            clock,
        }
    }

    /// Build a buffer from raw normalized sample values (±1.0 = full scale).
    #[must_use]
    pub fn from_samples(
        values: Vec<f32>,
        rate: SampleRate,
        bits: BitDepth,
        clock: ClockDomainId,
    ) -> Self {
        Self {
            values,
            rate,
            bits,
            clock,
        }
    }

    /// The sample rate of this stream.
    pub fn rate(&self) -> SampleRate {
        self.rate
    }

    /// The bit depth the samples were quantized at.
    pub fn bits(&self) -> BitDepth {
        self.bits
    }

    /// The clock domain (oscillator identity) that produced this stream.
    pub fn clock(&self) -> ClockDomainId {
        self.clock
    }

    /// Number of samples in the block.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether the block has no samples.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// The raw normalized samples as a slice — the hot-path read view.
    pub fn as_slice(&self) -> &[f32] {
        &self.values
    }

    /// The raw normalized samples as a mutable slice — the hot-path write view.
    pub fn as_mut_slice(&mut self) -> &mut [f32] {
        &mut self.values
    }

    /// Fill every sample with `value` (normalized, ±1.0 = full scale).
    pub fn fill(&mut self, value: f32) {
        self.values.fill(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt() -> (SampleRate, BitDepth, ClockDomainId) {
        (
            SampleRate::new(48_000.0),
            BitDepth::new(24),
            ClockDomainId::SINGLE,
        )
    }

    #[test]
    fn zeros_has_len_format_and_is_silent() {
        let (rate, bits, clock) = fmt();
        let buf = SampleBuffer::zeros(4, rate, bits, clock);
        assert_eq!(buf.len(), 4);
        assert!(!buf.is_empty());
        assert_eq!(buf.rate(), rate);
        assert_eq!(buf.bits(), bits);
        assert_eq!(buf.clock(), clock);
        assert!(buf.as_slice().iter().all(|&s| s.to_bits() == 0));
    }

    #[test]
    fn empty_buffer() {
        let (rate, bits, clock) = fmt();
        let buf = SampleBuffer::zeros(0, rate, bits, clock);
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
    }

    #[test]
    fn from_samples_preserves_values() {
        let (rate, bits, clock) = fmt();
        let buf = SampleBuffer::from_samples(vec![0.0, 0.5, -1.0], rate, bits, clock);
        assert_eq!(buf.as_slice(), &[0.0, 0.5, -1.0]);
    }

    #[test]
    fn mut_slice_and_fill_are_the_write_views() {
        let (rate, bits, clock) = fmt();
        let mut buf = SampleBuffer::zeros(3, rate, bits, clock);
        for (i, s) in buf.as_mut_slice().iter_mut().enumerate() {
            *s = i as f32 * 0.25;
        }
        assert_eq!(buf.as_slice(), &[0.0, 0.25, 0.5]);

        buf.fill(-1.0);
        assert!(buf.as_slice().iter().all(|&s| s == -1.0));
    }
}
