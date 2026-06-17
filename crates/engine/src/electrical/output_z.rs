//! A device output's electrical face: its output impedance.

use super::Ohms;

/// The fixed electrical face of a device output: its output impedance, `Zout`.
///
/// The static half of a Thévenin output ([`Thevenin`](super::Thevenin)`{ v_src, z_out }`): an
/// output port is an ideal voltage source in series with this impedance. The `v_src` is the
/// per-block signal the device *produces*; `OutputZ` is the fixed electrical property it
/// *declares* up front. Mirrors [`InputZ`](super::InputZ) so the two device faces read in
/// parallel — a passive input simply *is* its `InputZ`, while an active output declares its
/// `OutputZ` and drives a `v_src` through it.
///
/// `Zout` sets how stiff a source the output is:
/// - low `Zout` (≪ load `Zin`) → **bridging**: it drives the load with negligible loss;
/// - `Zout` = load `Zin` → **matching**: −6 dB into the load.
///
/// Single-conductor for now; balanced (V+/V−) outputs arrive in Story 1.5.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutputZ {
    z_out: Ohms,
}

impl OutputZ {
    /// An output presenting impedance `z_out`.
    #[must_use]
    pub fn new(z_out: Ohms) -> Self {
        Self { z_out }
    }

    /// The output impedance, `Zout`.
    pub fn z_out(self) -> Ohms {
        self.z_out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_impedance() {
        let face = OutputZ::new(Ohms::new(150.0));
        assert_eq!(face.z_out(), Ohms::new(150.0));
    }
}
