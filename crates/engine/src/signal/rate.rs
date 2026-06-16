//! The analog-domain clock.

/// The engine's one fundamental clock: the oversampled rate that stands in for
/// "continuous" in the analog domain.
///
/// It is always a constructor parameter, never a constant. There is deliberately **no**
/// global oversample factor and **no** global digital base rate — digital sample rates
/// are per-converter and emerge at the AD (Story 1.6). Crossing any clock boundary is a
/// resample; nothing here encodes a fixed analog↔digital ratio.
///
/// Stored in hertz as `f64` (accumulator precision; time math derives from it).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct AnalogRate {
    hz: f64,
}

impl AnalogRate {
    /// Create a rate from a frequency in hertz.
    ///
    /// # Panics
    /// Panics if `hz` is not finite and strictly positive. The rate is fixed at
    /// construction time — never on the hot path — so an invalid value is a setup bug,
    /// caught loudly here rather than producing silent `NaN`/`inf` downstream.
    #[must_use]
    pub fn new(hz: f64) -> Self {
        assert!(
            hz.is_finite() && hz > 0.0,
            "AnalogRate must be finite and > 0, got {hz}"
        );
        Self { hz }
    }

    /// The rate in hertz.
    pub fn as_hz(self) -> f64 {
        self.hz
    }

    /// The time between adjacent samples, in seconds (`1 / rate`).
    pub fn seconds_per_sample(self) -> f64 {
        1.0 / self.hz
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn round_trips_hz() {
        let rate = AnalogRate::new(384_000.0);
        assert_relative_eq!(rate.as_hz(), 384_000.0);
    }

    #[test]
    fn seconds_per_sample_is_reciprocal() {
        // 384 kHz => 1 / 384000 ≈ 2.604166e-6 s between samples.
        let rate = AnalogRate::new(384_000.0);
        assert_relative_eq!(rate.seconds_per_sample(), 1.0 / 384_000.0, epsilon = 1e-15);
    }

    #[test]
    #[should_panic(expected = "finite and > 0")]
    fn rejects_zero() {
        let _ = AnalogRate::new(0.0);
    }

    #[test]
    #[should_panic(expected = "finite and > 0")]
    fn rejects_negative() {
        let _ = AnalogRate::new(-48_000.0);
    }

    #[test]
    #[should_panic(expected = "finite and > 0")]
    fn rejects_nan() {
        let _ = AnalogRate::new(f64::NAN);
    }
}
