//! A block of acoustic-domain sound pressure.

use super::{AnalogRate, Pascals};

/// A block of sound pressure, in pascals, sampled at an [`AnalogRate`].
///
/// The acoustic-domain peer of [`VoltageBuffer`](super::VoltageBuffer): pressure is a real,
/// oversampled waveform against the same analog continuum as volts — **not** a digital sample
/// stream, so it carries an [`AnalogRate`], never a per-converter `SampleRate`. A microphone (the
/// acoustic→analog transducer) is the only place a `PressureBuffer` becomes a
/// [`VoltageBuffer`](super::VoltageBuffer).
///
/// Values are stored as raw contiguous `f32` (compact and SIMD-friendly for the hot path);
/// element access is offered as [`Pascals`] for type safety at the edges. Linear values only —
/// dB SPL is a measurement unit, never a storage format. Mono: air is modelled ideally, with no
/// balanced-leg or channel multiplicity. All allocation happens at construction — the processing
/// path mutates an existing buffer via [`PressureBuffer::as_mut_slice`] and never allocates.
#[derive(Debug, Clone, PartialEq)]
pub struct PressureBuffer {
    /// Linear pascals, one element per sample; length is the block length.
    values: Vec<f32>,
    rate: AnalogRate,
}

impl PressureBuffer {
    /// A zero-filled buffer of `len` samples at `rate`.
    #[must_use]
    pub fn zeros(len: usize, rate: AnalogRate) -> Self {
        Self {
            values: vec![0.0; len],
            rate,
        }
    }

    /// Build a buffer from raw pascal values.
    #[must_use]
    pub fn from_pascals(values: Vec<f32>, rate: AnalogRate) -> Self {
        Self { values, rate }
    }

    /// The analog rate this buffer is sampled at.
    pub fn rate(&self) -> AnalogRate {
        self.rate
    }

    /// Number of samples in the block.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether the block has no samples.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// The raw pascal values as a slice — the hot-path read view.
    pub fn as_slice(&self) -> &[f32] {
        &self.values
    }

    /// The raw pascal values as a mutable slice — the hot-path write view.
    pub fn as_mut_slice(&mut self) -> &mut [f32] {
        &mut self.values
    }

    /// The pressure at sample index `i`.
    ///
    /// # Panics
    /// Panics if `i` is out of bounds. This is a setup/test convenience; hot-path code
    /// iterates [`as_slice`](Self::as_slice) / [`as_mut_slice`](Self::as_mut_slice).
    pub fn get(&self, i: usize) -> Pascals {
        Pascals::new(self.values[i])
    }

    /// Set the pressure at sample index `i`.
    ///
    /// # Panics
    /// Panics if `i` is out of bounds (setup/test convenience; see [`get`](Self::get)).
    pub fn set(&mut self, i: usize, p: Pascals) {
        self.values[i] = p.get();
    }

    /// Fill every sample with `p`.
    pub fn fill(&mut self, p: Pascals) {
        self.values.fill(p.get());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    #[test]
    fn zeros_has_len_and_is_silent() {
        let buf = PressureBuffer::zeros(4, rate());
        assert_eq!(buf.len(), 4);
        assert!(!buf.is_empty());
        assert_eq!(buf.rate(), rate());
        assert!(buf.as_slice().iter().all(|&p| p.to_bits() == 0));
        assert_eq!(buf.get(0), Pascals::ZERO);
    }

    #[test]
    fn empty_buffer() {
        let buf = PressureBuffer::zeros(0, rate());
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
    }

    #[test]
    fn from_pascals_preserves_values() {
        let buf = PressureBuffer::from_pascals(vec![0.0, 1.0, -2.0], rate());
        assert_eq!(buf.get(0), Pascals::new(0.0));
        assert_eq!(buf.get(1), Pascals::new(1.0));
        assert_eq!(buf.get(2), Pascals::new(-2.0));
    }

    #[test]
    fn set_and_fill() {
        let mut buf = PressureBuffer::zeros(3, rate());
        buf.set(1, Pascals::new(1.5));
        assert_eq!(buf.get(1), Pascals::new(1.5));

        buf.fill(Pascals::new(-1.0));
        assert!(
            buf.as_slice()
                .iter()
                .all(|&p| (p + 1.0).abs() < f32::EPSILON)
        );
    }

    #[test]
    fn mut_slice_is_the_write_view() {
        let mut buf = PressureBuffer::zeros(3, rate());
        for (i, s) in buf.as_mut_slice().iter_mut().enumerate() {
            *s = i as f32;
        }
        assert_eq!(buf.as_slice(), &[0.0, 1.0, 2.0]);
    }
}
