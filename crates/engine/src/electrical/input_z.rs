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
/// # Conductors
/// An input is **unbalanced** ([`new`](Self::new), one conductor referenced to ground) or
/// **balanced** ([`balanced`](Self::balanced), two conductors V+/V− carrying a differential
/// signal). For a balanced input, `z_in` is the **differential** input impedance (across the
/// pair), and the divider solve treats it as such — each conductor of an edge is scaled by the
/// differential divider gain. Unbalanced is the degenerate one-conductor case (the cold leg is
/// ground), so a balanced→unbalanced mismatch isn't special-cased: it's just two ports with
/// different conductor counts (rejected at compile until adapters land — see Story 1.5 notes).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InputZ {
    z_in: Ohms,
    conductors: usize,
}

impl InputZ {
    /// An **unbalanced** (single-conductor) input presenting impedance `z_in`.
    #[must_use]
    pub fn new(z_in: Ohms) -> Self {
        Self {
            z_in,
            conductors: 1,
        }
    }

    /// A **balanced** (two-conductor, V+/V−) input presenting **differential** impedance `z_in`.
    #[must_use]
    pub fn balanced(z_in: Ohms) -> Self {
        Self {
            z_in,
            conductors: 2,
        }
    }

    /// An input with an explicit conductor count `n`, used by the per-conductor lift to mint a
    /// balanced face from a single-conductor node's. Internal — the public API is
    /// [`new`](Self::new) / [`balanced`](Self::balanced).
    #[must_use]
    pub(crate) fn with_conductors(z_in: Ohms, n: usize) -> Self {
        Self {
            z_in,
            conductors: n,
        }
    }

    /// The input impedance, `Zin` (the **differential** impedance for a balanced input).
    pub fn z_in(self) -> Ohms {
        self.z_in
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
        let load = InputZ::new(Ohms::new(10_000.0));
        assert_eq!(load.z_in(), Ohms::new(10_000.0));
    }

    #[test]
    fn new_is_unbalanced_balanced_is_two_conductor() {
        assert_eq!(InputZ::new(Ohms::new(10_000.0)).conductors(), 1);
        assert_eq!(InputZ::balanced(Ohms::new(10_000.0)).conductors(), 2);
    }
}
