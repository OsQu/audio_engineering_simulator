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
/// # Conductors
/// An output is **unbalanced** ([`new`](Self::new), one conductor referenced to ground) or
/// **balanced** ([`balanced`](Self::balanced), two conductors V+/V− driven differentially). For
/// a balanced output, `z_out` is the **differential** output impedance, mirroring
/// [`InputZ::balanced`](super::InputZ::balanced) on the load side.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutputZ {
    z_out: Ohms,
    conductors: usize,
}

impl OutputZ {
    /// An **unbalanced** (single-conductor) output presenting impedance `z_out`.
    #[must_use]
    pub fn new(z_out: Ohms) -> Self {
        Self {
            z_out,
            conductors: 1,
        }
    }

    /// A **balanced** (two-conductor, V+/V−) output presenting **differential** impedance `z_out`.
    #[must_use]
    pub fn balanced(z_out: Ohms) -> Self {
        Self {
            z_out,
            conductors: 2,
        }
    }

    /// An output with an explicit conductor count `n`, used by the per-conductor lift to mint a
    /// balanced face from a single-conductor node's. Internal — the public API is
    /// [`new`](Self::new) / [`balanced`](Self::balanced).
    #[must_use]
    pub(crate) fn with_conductors(z_out: Ohms, n: usize) -> Self {
        Self {
            z_out,
            conductors: n,
        }
    }

    /// The output impedance, `Zout` (the **differential** impedance for a balanced output).
    pub fn z_out(self) -> Ohms {
        self.z_out
    }

    /// The number of conductors: 1 (unbalanced) or 2 (balanced).
    pub fn conductors(self) -> usize {
        self.conductors
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

    #[test]
    fn new_is_unbalanced_balanced_is_two_conductor() {
        assert_eq!(OutputZ::new(Ohms::new(150.0)).conductors(), 1);
        assert_eq!(OutputZ::balanced(Ohms::new(200.0)).conductors(), 2);
    }
}
