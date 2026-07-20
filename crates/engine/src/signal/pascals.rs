//! Sound-pressure scalar.

use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// A scalar sound pressure, in pascals. Linear — never decibels.
///
/// The acoustic-domain peer of [`Volts`](crate::Volts): a distinct newtype (not a bare `f32`)
/// so a sound pressure can't be silently mixed with a voltage or a dimensionless gain. The
/// acoustic and analog domains only meet at a **microphone** (the acoustic→analog transducer),
/// exactly as analog and digital only meet at an AD/DA. Stored as `f32` per the module's scalar
/// policy.
///
/// Arithmetic models the physics: pressures add and subtract (superposition of sound fields) and
/// scale by a dimensionless gain (`* f32`), but two pressures do not multiply into pascals. Sound
/// pressure level (dB SPL) is a *measurement* of this value — see
/// [`spl_to_pascals`](crate::spl_to_pascals) / [`pascals_to_spl`](crate::pascals_to_spl) — never a
/// storage format.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Pascals(f32);

impl Pascals {
    /// Zero pressure (silence).
    pub const ZERO: Pascals = Pascals(0.0);

    /// Wrap a raw value, in pascals.
    #[must_use]
    pub const fn new(pascals: f32) -> Self {
        Pascals(pascals)
    }

    /// The underlying value, in pascals.
    pub const fn get(self) -> f32 {
        self.0
    }

    /// Absolute value.
    #[must_use]
    pub fn abs(self) -> Self {
        Pascals(self.0.abs())
    }
}

impl Add for Pascals {
    type Output = Pascals;
    fn add(self, rhs: Pascals) -> Pascals {
        Pascals(self.0 + rhs.0)
    }
}

impl Sub for Pascals {
    type Output = Pascals;
    fn sub(self, rhs: Pascals) -> Pascals {
        Pascals(self.0 - rhs.0)
    }
}

impl Neg for Pascals {
    type Output = Pascals;
    fn neg(self) -> Pascals {
        Pascals(-self.0)
    }
}

/// Scale by a dimensionless gain.
impl Mul<f32> for Pascals {
    type Output = Pascals;
    fn mul(self, gain: f32) -> Pascals {
        Pascals(self.0 * gain)
    }
}

/// Scale by a dimensionless gain (gain on the left).
impl Mul<Pascals> for f32 {
    type Output = Pascals;
    fn mul(self, p: Pascals) -> Pascals {
        Pascals(self * p.0)
    }
}

/// Divide by a dimensionless scalar.
impl Div<f32> for Pascals {
    type Output = Pascals;
    fn div(self, divisor: f32) -> Pascals {
        Pascals(self.0 / divisor)
    }
}

impl AddAssign for Pascals {
    fn add_assign(&mut self, rhs: Pascals) {
        self.0 += rhs.0;
    }
}

impl SubAssign for Pascals {
    fn sub_assign(&mut self, rhs: Pascals) {
        self.0 -= rhs.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn new_and_get_round_trip() {
        // 1 Pa is the 94 dB SPL calibration anchor.
        assert_relative_eq!(Pascals::new(1.0).get(), 1.0);
    }

    #[test]
    fn zero_constant() {
        assert_eq!(Pascals::ZERO, Pascals::new(0.0));
    }

    #[test]
    fn adds_and_subtracts() {
        // Two sound fields superpose (add); difference is the same linear operation.
        assert_eq!(Pascals::new(1.0) + Pascals::new(0.5), Pascals::new(1.5));
        assert_eq!(Pascals::new(1.0) - Pascals::new(0.25), Pascals::new(0.75));
    }

    #[test]
    fn negates() {
        assert_eq!(-Pascals::new(2.0), Pascals::new(-2.0));
        assert_eq!(Pascals::new(-3.5).abs(), Pascals::new(3.5));
    }

    #[test]
    fn scales_by_gain_either_side() {
        // A gain of 2× on a 0.5 Pa field => 1.0 Pa, regardless of operand order.
        assert_eq!(Pascals::new(0.5) * 2.0, Pascals::new(1.0));
        assert_eq!(2.0 * Pascals::new(0.5), Pascals::new(1.0));
        assert_eq!(Pascals::new(1.0) / 4.0, Pascals::new(0.25));
    }

    #[test]
    fn assign_ops() {
        let mut p = Pascals::new(1.0);
        p += Pascals::new(0.5);
        p -= Pascals::new(0.25);
        assert_eq!(p, Pascals::new(1.25));
    }
}
