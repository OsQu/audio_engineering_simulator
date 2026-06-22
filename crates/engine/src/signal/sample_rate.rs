//! A digital converter's sample rate.

/// The sample rate of a digital-audio stream, in hertz.
///
/// A **distinct** newtype from [`AnalogRate`](crate::AnalogRate) on purpose: the analog
/// continuous-proxy clock and a converter's digital rate are different domains that only ever
/// meet at an AD/DA, and the type system must refuse to mix them. There is no global digital
/// rate — each converter stamps its own onto the samples it produces (Story 1.6).
///
/// Stored in hertz as `f64`: the analog rate it divides is `f64`, and the decimation ratio
/// `analog / digital` must be computed exactly to validate the integer-divide constraint.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct SampleRate {
    hz: f64,
}

impl SampleRate {
    /// Create a sample rate from a frequency in hertz.
    ///
    /// # Panics
    /// Panics unless `hz` is finite and strictly positive. The rate is fixed at construction,
    /// never on the hot path, so an invalid value is a setup bug caught loudly here.
    #[must_use]
    pub fn new(hz: f64) -> Self {
        assert!(
            hz.is_finite() && hz > 0.0,
            "SampleRate must be finite and > 0, got {hz}"
        );
        Self { hz }
    }

    /// The rate in hertz.
    pub fn as_hz(self) -> f64 {
        self.hz
    }

    /// The Nyquist frequency, in hertz (`rate / 2`) — the highest frequency this stream can
    /// represent, and the edge the AD's anti-alias filter must protect (Story 1.6).
    pub fn nyquist_hz(self) -> f64 {
        self.hz * 0.5
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn round_trips_hz() {
        assert_relative_eq!(SampleRate::new(48_000.0).as_hz(), 48_000.0);
    }

    #[test]
    fn nyquist_is_half() {
        // 48 kHz stream => 24 kHz Nyquist.
        assert_relative_eq!(SampleRate::new(48_000.0).nyquist_hz(), 24_000.0);
    }

    #[test]
    #[should_panic(expected = "finite and > 0")]
    fn rejects_zero() {
        let _ = SampleRate::new(0.0);
    }

    #[test]
    #[should_panic(expected = "finite and > 0")]
    fn rejects_nan() {
        let _ = SampleRate::new(f64::NAN);
    }
}
