//! A device output's electrical face: a Thévenin source.

use super::Ohms;
use crate::signal::Volts;

/// The Thévenin equivalent of a device output: an ideal voltage source in series with an
/// output impedance (PROJECT_PLAN §5.3). See <https://en.wikipedia.org/wiki/Th%C3%A9venin%27s_theorem>
///
/// `v_src` is the **open-circuit** source voltage — what the output would produce into an
/// infinite load. The voltage a real receiver sees is lower, set by the voltage divider the
/// source, cable, and load form (the `divider_gain` solve, Story 1.2.2). Holding a scalar
/// `v_src` here is a setup/test convenience; in the running engine the source voltage is a
/// per-sample signal while `z_out` stays the fixed electrical property.
///
/// Single-conductor for now; balanced (V+/V−) outputs arrive in Story 1.5.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thevenin {
    /// Open-circuit (unloaded) source voltage.
    v_src: Volts,
    /// Series output impedance, `Zout`.
    z_out: Ohms,
}

impl Thevenin {
    /// A source with open-circuit voltage `v_src` and output impedance `z_out`.
    #[must_use]
    pub fn new(v_src: Volts, z_out: Ohms) -> Self {
        Self { v_src, z_out }
    }

    /// The open-circuit (unloaded) source voltage.
    pub fn v_src(self) -> Volts {
        self.v_src
    }

    /// The series output impedance, `Zout`.
    pub fn z_out(self) -> Ohms {
        self.z_out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_voltage_and_impedance() {
        let src = Thevenin::new(Volts::new(1.23), Ohms::new(150.0));
        assert_eq!(src.v_src(), Volts::new(1.23));
        assert_eq!(src.z_out(), Ohms::new(150.0));
    }

    #[test]
    fn an_ideal_source_has_zero_output_impedance() {
        let src = Thevenin::new(Volts::new(0.775), Ohms::ZERO);
        assert_eq!(src.z_out(), Ohms::ZERO);
    }
}
