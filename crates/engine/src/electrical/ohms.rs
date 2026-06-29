//! Impedance scalar.

use std::ops::Add;

/// An electrical impedance, in ohms. Real (resistive) for now.
///
/// A distinct newtype (not a bare `f32`) so impedances can't be silently mixed with
/// voltages or dimensionless gains. Stored as `f32` per the engine's scalar policy.
///
/// Impedance is **resistive only** — a single real number. Reactive (frequency-dependent)
/// impedance is deliberately *not* modeled: it's the door to emergent cross-device resonance,
/// opened later if a reactive device earns it. The cable's one reactive element (its shunt
/// capacitance) is handled separately as a one-pole filter, not as a complex `Ohms`.
///
/// Arithmetic models the network: impedances in series **add** (`+`); impedances in
/// parallel combine via [`Ohms::parallel`] (fan-out of loads).
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Ohms(f32);

impl Ohms {
    /// Zero ohms (an ideal, lossless connection — e.g. a perfect source or no cable).
    pub const ZERO: Ohms = Ohms(0.0);

    /// Wrap a raw value, in ohms.
    ///
    /// # Panics
    /// Panics if `ohms` is not finite or is negative. Impedances are fixed at setup time,
    /// never on the hot path, so an invalid value is a construction bug — caught loudly
    /// here rather than producing silent `NaN`/`inf` in the divider solve downstream.
    #[must_use]
    pub fn new(ohms: f32) -> Self {
        assert!(
            ohms.is_finite() && ohms >= 0.0,
            "Ohms must be finite and >= 0, got {ohms}"
        );
        Ohms(ohms)
    }

    /// The underlying value, in ohms.
    pub const fn get(self) -> f32 {
        self.0
    }

    /// Two impedances in parallel: `(a·b)/(a+b)`.
    ///
    /// This is fan-out — splitting one output into several inputs presents their input
    /// impedances in parallel. The result is always ≤ the smaller operand (adding a load
    /// can only lower the combined impedance). Two zero-ohm shorts combine to zero rather
    /// than `0/0`.
    #[must_use]
    pub fn parallel(self, other: Ohms) -> Ohms {
        let sum = self.0 + other.0;
        if sum == 0.0 {
            // Both are zero ohms — a short in parallel with a short is still a short.
            Ohms::ZERO
        } else {
            Ohms(self.0 * other.0 / sum)
        }
    }
}

/// Series impedances add.
impl Add for Ohms {
    type Output = Ohms;
    fn add(self, rhs: Ohms) -> Ohms {
        Ohms(self.0 + rhs.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn new_and_get_round_trip() {
        assert_relative_eq!(Ohms::new(600.0).get(), 600.0);
    }

    #[test]
    fn zero_constant() {
        assert_eq!(Ohms::ZERO, Ohms::new(0.0));
    }

    #[test]
    fn series_adds() {
        // Zout + Zcable in the divider: 100 Ω + 50 Ω = 150 Ω.
        assert_eq!(Ohms::new(100.0) + Ohms::new(50.0), Ohms::new(150.0));
    }

    #[test]
    fn parallel_of_two_equal_is_half() {
        // Two 600 Ω loads in parallel => 600·600 / 1200 = 300 Ω.
        assert_relative_eq!(Ohms::new(600.0).parallel(Ohms::new(600.0)).get(), 300.0);
    }

    #[test]
    fn parallel_with_a_much_larger_is_near_the_smaller() {
        // 1 kΩ ∥ 1 MΩ ≈ 999 Ω — the big one barely loads the small one.
        // 1000·1_000_000 / 1_001_000 = 999.000999…
        assert_relative_eq!(
            Ohms::new(1_000.0).parallel(Ohms::new(1_000_000.0)).get(),
            999.001,
            epsilon = 1e-2
        );
    }

    #[test]
    fn parallel_is_commutative() {
        let a = Ohms::new(330.0);
        let b = Ohms::new(470.0);
        assert_relative_eq!(a.parallel(b).get(), b.parallel(a).get());
    }

    #[test]
    fn parallel_with_zero_is_zero() {
        // A dead short in parallel pulls the combination to zero.
        assert_eq!(Ohms::new(600.0).parallel(Ohms::ZERO), Ohms::ZERO);
        assert_eq!(Ohms::ZERO.parallel(Ohms::ZERO), Ohms::ZERO);
    }

    #[test]
    #[should_panic(expected = "finite and >= 0")]
    fn rejects_negative() {
        let _ = Ohms::new(-1.0);
    }

    #[test]
    #[should_panic(expected = "finite and >= 0")]
    fn rejects_nan() {
        let _ = Ohms::new(f32::NAN);
    }
}
