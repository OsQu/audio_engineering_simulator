//! Voltage scalar.

use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// A scalar voltage, in volts. Linear — never decibels.
///
/// A distinct newtype (not a bare `f32`) so voltages can't be silently mixed with
/// digital dBFS samples or dimensionless gains; the analog and digital domains only meet
/// at an AD/DA converter. Stored as `f32` per the module's scalar policy.
///
/// Arithmetic models the physics: voltages add and subtract (series sources, difference
/// at a balanced receiver) and scale by a dimensionless gain (`* f32`), but two voltages
/// do not multiply into volts.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Volts(f32);

impl Volts {
    /// Zero volts.
    pub const ZERO: Volts = Volts(0.0);

    /// Wrap a raw value, in volts.
    #[must_use]
    pub const fn new(volts: f32) -> Self {
        Volts(volts)
    }

    /// The underlying value, in volts.
    pub const fn get(self) -> f32 {
        self.0
    }

    /// Absolute value.
    #[must_use]
    pub fn abs(self) -> Self {
        Volts(self.0.abs())
    }
}

impl Add for Volts {
    type Output = Volts;
    fn add(self, rhs: Volts) -> Volts {
        Volts(self.0 + rhs.0)
    }
}

impl Sub for Volts {
    type Output = Volts;
    fn sub(self, rhs: Volts) -> Volts {
        Volts(self.0 - rhs.0)
    }
}

impl Neg for Volts {
    type Output = Volts;
    fn neg(self) -> Volts {
        Volts(-self.0)
    }
}

/// Scale by a dimensionless gain.
impl Mul<f32> for Volts {
    type Output = Volts;
    fn mul(self, gain: f32) -> Volts {
        Volts(self.0 * gain)
    }
}

/// Scale by a dimensionless gain (gain on the left).
impl Mul<Volts> for f32 {
    type Output = Volts;
    fn mul(self, v: Volts) -> Volts {
        Volts(self * v.0)
    }
}

/// Divide by a dimensionless scalar.
impl Div<f32> for Volts {
    type Output = Volts;
    fn div(self, divisor: f32) -> Volts {
        Volts(self.0 / divisor)
    }
}

impl AddAssign for Volts {
    fn add_assign(&mut self, rhs: Volts) {
        self.0 += rhs.0;
    }
}

impl SubAssign for Volts {
    fn sub_assign(&mut self, rhs: Volts) {
        self.0 -= rhs.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn new_and_get_round_trip() {
        assert_relative_eq!(Volts::new(0.775).get(), 0.775);
    }

    #[test]
    fn zero_constant() {
        assert_eq!(Volts::ZERO, Volts::new(0.0));
    }

    #[test]
    fn adds_and_subtracts() {
        // Series sources sum; difference is the balanced-receiver case.
        assert_eq!(Volts::new(1.0) + Volts::new(0.5), Volts::new(1.5));
        assert_eq!(Volts::new(1.0) - Volts::new(0.25), Volts::new(0.75));
    }

    #[test]
    fn negates() {
        assert_eq!(-Volts::new(2.0), Volts::new(-2.0));
        assert_eq!(Volts::new(-3.5).abs(), Volts::new(3.5));
    }

    #[test]
    fn scales_by_gain_either_side() {
        // A gain of 2× on a 0.5 V signal => 1.0 V, regardless of operand order.
        assert_eq!(Volts::new(0.5) * 2.0, Volts::new(1.0));
        assert_eq!(2.0 * Volts::new(0.5), Volts::new(1.0));
        assert_eq!(Volts::new(1.0) / 4.0, Volts::new(0.25));
    }

    #[test]
    fn assign_ops() {
        let mut v = Volts::new(1.0);
        v += Volts::new(0.5);
        v -= Volts::new(0.25);
        assert_eq!(v, Volts::new(1.25));
    }
}
