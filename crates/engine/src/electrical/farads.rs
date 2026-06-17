//! Farads scalar

/// Capacitance, in farads.
///
/// A distinct newtype like [`Ohms`](super::Ohms), stored as `f32`. Real-cable values are tiny —
/// picofarads to nanofarads (instrument cable is roughly 100 pF/metre) — but `f32`'s
/// exponent range holds them comfortably; the filter math that consumes it runs in `f64`.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Farads(f32);

impl Farads {
    /// Zero farads (no capacitance — the cable becomes purely resistive).
    pub const ZERO: Farads = Farads(0.0);

    /// Wrap a raw value, in farads.
    ///
    /// # Panics
    /// Panics if `farads` is not finite or is negative (a setup bug, like [`Ohms::new`](super::Ohms::new)).
    #[must_use]
    pub fn new(farads: f32) -> Self {
        assert!(
            farads.is_finite() && farads >= 0.0,
            "Farads must be finite and >= 0, got {farads}"
        );
        Farads(farads)
    }

    /// The underlying value, in farads.
    pub const fn get(self) -> f32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    #[should_panic(expected = "finite and >= 0")]
    fn farads_rejects_negative() {
        let _ = Farads::new(-1e-9);
    }

    #[test]
    fn farads_round_trips_and_rejects_bad_values() {
        assert_relative_eq!(Farads::new(1e-9).get(), 1e-9);
        assert_eq!(Farads::ZERO, Farads::new(0.0));
    }
}
