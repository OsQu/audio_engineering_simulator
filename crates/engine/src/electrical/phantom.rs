//! Phantom-power declarations: the supply and load faces of the DC bias network.
//!
//! +48 V phantom is a **static DC network**: a supply rail behind fixed per-leg feed resistors on
//! the receiving device's input, and a DC load inside the mic drawing through its output pair. Both
//! ends are **circuit-topology declarations** — the same class of port-fact as
//! [`InputZ`]/[`OutputZ`](super::OutputZ), never labels on the signal. Because the
//! network is linear and stationary, superposition splits the problem (`osku_physics_concepts.md`
//! §17): the DC **operating point** is solved once at compile with the same local divider as the
//! audio solve, and the per-sample AC rides on top of the solved pedestal.

use super::{InputZ, Ohms, divider_gain};
use crate::signal::Volts;

/// A phantom-power **supply** declared on an analog *input* port — the device feeds DC back up the
/// same wire pair it receives audio from (a preamp's mic input, IEC 61938 "P48": +48 V behind
/// 6.8 kΩ per leg).
///
/// `engaged` is part of the declaration: the feed network exists either way (the 48V switch), and
/// which state it's in is **structural** — toggling it changes the DC topology, so it recompiles,
/// like repatching. A supply facing a producer that declares no [`PhantomLoad`] resolves nothing —
/// the real-world "mostly harmless" case (48 V into a line output), simplified to a no-op.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhantomSupply {
    volts: Volts,
    feed_per_leg: Ohms,
    engaged: bool,
}

impl PhantomSupply {
    /// A supply of `volts` behind `feed_per_leg` on each conductor, currently `engaged` or not.
    #[must_use]
    pub fn new(volts: Volts, feed_per_leg: Ohms, engaged: bool) -> Self {
        Self {
            volts,
            feed_per_leg,
            engaged,
        }
    }

    /// The supply rail voltage (+48 V for standard phantom).
    pub fn volts(self) -> Volts {
        self.volts
    }

    /// The per-leg feed resistance (6.8 kΩ for P48).
    pub fn feed_per_leg(self) -> Ohms {
        self.feed_per_leg
    }

    /// Whether the supply is switched on. Disengaged, it resolves every load to 0 V.
    pub fn engaged(self) -> bool {
        self.engaged
    }

    /// The feed network's resistance to **common-mode** current: the two per-leg resistors act in
    /// parallel (the DC draw returns equally through both conductors), so `feed_per_leg / 2` —
    /// 3.4 kΩ for P48.
    #[must_use]
    pub fn common_mode_feed(self) -> Ohms {
        Ohms::new(self.feed_per_leg.get() * 0.5)
    }

    /// The DC **operating point** at the load's terminals: the SPICE-`.OP` half of the
    /// superposition split, solved with the same local divider as the audio solve —
    ///
    /// ```text
    ///   V_dc = V_supply · Z_load / (feed/2 + R_cable + Z_load)
    /// ```
    ///
    /// Reference hand calc (§17): 48 V, 6.8 kΩ per leg, no cable, into a 12.7 kΩ load ⇒
    /// `48 · 12 700 / (3 400 + 0 + 12 700) = 37.86 V` — sag emerges from the divider. A
    /// **disengaged** supply delivers 0 V (total, so callers needn't special-case the switch).
    ///
    /// Compile-time only (never the hot path).
    #[must_use]
    pub fn terminal_volts(self, r_cable: Ohms, load: PhantomLoad) -> Volts {
        if !self.engaged {
            return Volts::ZERO;
        }
        self.volts * divider_gain(self.common_mode_feed(), r_cable, InputZ::new(load.z_dc()))
    }
}

/// A phantom-power **load** declared on an analog *output* port — the device draws its operating
/// current through the pair it emits audio on (a condenser mic's XLR output).
///
/// The load is a **constant resistance** (`z_dc`), keeping the operating-point solve a plain
/// divider (a constant-current model would make it iterative for no audible payoff). `v_min` is the
/// minimum terminal voltage the device's electronics run at — a threshold in the device itself, the
/// same species of device-internal physics as rail clipping, not a flag on the signal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhantomLoad {
    z_dc: Ohms,
    v_min: Volts,
}

impl PhantomLoad {
    /// A DC load of `z_dc` that operates at terminal voltages of `v_min` and above.
    #[must_use]
    pub fn new(z_dc: Ohms, v_min: Volts) -> Self {
        Self { z_dc, v_min }
    }

    /// The constant-resistance DC load.
    pub fn z_dc(self) -> Ohms {
        self.z_dc
    }

    /// The minimum terminal voltage the device operates at.
    pub fn v_min(self) -> Volts {
        self.v_min
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    /// The ~3 mA-class reference load from the plan: 12.7 kΩ, runs at ≥ 35 V.
    fn reference_load() -> PhantomLoad {
        PhantomLoad::new(Ohms::new(12_700.0), Volts::new(35.0))
    }

    fn p48(engaged: bool) -> PhantomSupply {
        PhantomSupply::new(Volts::new(48.0), Ohms::new(6_800.0), engaged)
    }

    #[test]
    fn common_mode_feed_is_the_per_leg_parallel() {
        // Two 6.8 kΩ legs in parallel for common-mode current: 6 800 / 2 = 3 400 Ω.
        assert_relative_eq!(p48(true).common_mode_feed().get(), 3_400.0, epsilon = 1e-3);
    }

    #[test]
    fn operating_point_matches_the_hand_calc() {
        // The §17 reference: 48 · 12 700 / (3 400 + 0 + 12 700) = 609 600 / 16 100 = 37.863 V.
        let v = p48(true).terminal_volts(Ohms::ZERO, reference_load());
        assert_relative_eq!(v.get(), 37.863, epsilon = 1e-3);
    }

    #[test]
    fn cable_resistance_deepens_the_sag() {
        // 100 Ω of cable joins the divider: 48 · 12 700 / (3 400 + 100 + 12 700) = 37.630 V.
        let v = p48(true).terminal_volts(Ohms::new(100.0), reference_load());
        assert_relative_eq!(v.get(), 37.630, epsilon = 1e-3);
    }

    #[test]
    fn disengaged_supply_delivers_zero() {
        let v = p48(false).terminal_volts(Ohms::ZERO, reference_load());
        assert_relative_eq!(v.get(), 0.0);
    }
}
