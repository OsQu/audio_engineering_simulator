//! A block of analog-domain voltage.

use super::{AnalogRate, Volts};

/// A block of single-conductor analog voltage, in volts, sampled at an [`AnalogRate`].
///
/// Values are stored as raw contiguous `f32` (compact and SIMD-friendly for the hot
/// path); element access is offered as [`Volts`] for type safety at the edges. Linear
/// values only — dB is a measurement unit, never a storage format.
///
/// Single-conductor: a balanced (V+/V−) line is two of these, one buffer per leg. All
/// allocation happens at construction — the processing path mutates an existing buffer via
/// [`VoltageBuffer::as_mut_slice`] and never allocates.
#[derive(Debug, Clone, PartialEq)]
pub struct VoltageBuffer {
    /// Linear volts, one element per sample; length is the block length.
    values: Vec<f32>,
    rate: AnalogRate,
}

impl VoltageBuffer {
    /// A zero-filled buffer of `len` samples at `rate`.
    #[must_use]
    pub fn zeros(len: usize, rate: AnalogRate) -> Self {
        Self {
            values: vec![0.0; len],
            rate,
        }
    }

    /// Build a buffer from raw volt values.
    #[must_use]
    pub fn from_volts(values: Vec<f32>, rate: AnalogRate) -> Self {
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

    /// The raw volt values as a slice — the hot-path read view.
    pub fn as_slice(&self) -> &[f32] {
        &self.values
    }

    /// The raw volt values as a mutable slice — the hot-path write view.
    pub fn as_mut_slice(&mut self) -> &mut [f32] {
        &mut self.values
    }

    /// The voltage at sample index `i`.
    ///
    /// # Panics
    /// Panics if `i` is out of bounds. This is a setup/test convenience; hot-path code
    /// iterates [`as_slice`](Self::as_slice) / [`as_mut_slice`](Self::as_mut_slice).
    pub fn get(&self, i: usize) -> Volts {
        Volts::new(self.values[i])
    }

    /// Set the voltage at sample index `i`.
    ///
    /// # Panics
    /// Panics if `i` is out of bounds (setup/test convenience; see [`get`](Self::get)).
    pub fn set(&mut self, i: usize, v: Volts) {
        self.values[i] = v.get();
    }

    /// Fill every sample with `v`.
    pub fn fill(&mut self, v: Volts) {
        self.values.fill(v.get());
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
        let buf = VoltageBuffer::zeros(4, rate());
        assert_eq!(buf.len(), 4);
        assert!(!buf.is_empty());
        assert_eq!(buf.rate(), rate());
        assert!(buf.as_slice().iter().all(|&v| v.to_bits() == 0));
        assert_eq!(buf.get(0), Volts::ZERO);
    }

    #[test]
    fn empty_buffer() {
        let buf = VoltageBuffer::zeros(0, rate());
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
    }

    #[test]
    fn from_volts_preserves_values() {
        let buf = VoltageBuffer::from_volts(vec![0.0, 1.0, -2.0], rate());
        assert_eq!(buf.get(0), Volts::new(0.0));
        assert_eq!(buf.get(1), Volts::new(1.0));
        assert_eq!(buf.get(2), Volts::new(-2.0));
    }

    #[test]
    fn set_and_fill() {
        let mut buf = VoltageBuffer::zeros(3, rate());
        buf.set(1, Volts::new(1.5));
        assert_eq!(buf.get(1), Volts::new(1.5));

        buf.fill(Volts::new(-1.0));
        assert!(
            buf.as_slice()
                .iter()
                .all(|&v| (v + 1.0).abs() < f32::EPSILON)
        );
    }

    #[test]
    fn mut_slice_is_the_write_view() {
        let mut buf = VoltageBuffer::zeros(3, rate());
        for (i, s) in buf.as_mut_slice().iter_mut().enumerate() {
            *s = i as f32;
        }
        assert_eq!(buf.as_slice(), &[0.0, 1.0, 2.0]);
    }
}
