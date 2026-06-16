//! A device input's electrical face: its input impedance.

use super::Ohms;

/// The electrical face of a device input: its input impedance, `Zin`.
///
/// Named `InputZ`, **not** `Port`: `Port` is reserved for the Story 1.3 graph connection
/// point (typed, directional, carrying a signal), which will *contain* an `InputZ` on its
/// input side and a [`Thevenin`](super::Thevenin) on its output side. Keeping the electrical
/// description separate from the graph wiring keeps the layering clean.
///
/// `Zin` sets how hard a load the input presents:
/// - high `Zin` (≫ source `Zout`) → **bridging**: the input barely loads the source,
///   negligible loss;
/// - `Zin` = source `Zout` → **matching**: −6 dB.
///
/// Single-conductor for now; balanced (V+/V−) inputs arrive in Story 1.5.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InputZ {
    z_in: Ohms,
}

impl InputZ {
    /// An input presenting impedance `z_in`.
    #[must_use]
    pub fn new(z_in: Ohms) -> Self {
        Self { z_in }
    }

    /// The input impedance, `Zin`.
    pub fn z_in(self) -> Ohms {
        self.z_in
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_impedance() {
        let load = InputZ::new(Ohms::new(10_000.0));
        assert_eq!(load.z_in(), Ohms::new(10_000.0));
    }
}
