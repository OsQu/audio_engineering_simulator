//! The local voltage-divider solve.

use super::{InputZ, Ohms};

/// The dimensionless gain a source sees into a load across a cable — the local solve
/// (PROJECT_PLAN §5.3):
///
/// ```text
///   gain = Zin / (Zout + Zcable + Zin)        V_in = V_src · gain
/// ```
///
/// Returns a **gain factor**, not a voltage: it depends only on impedances, so it's
/// constant for a given connection and computed once (at `compile`), while the per-sample
/// `v_src` is multiplied in by the caller. `z_cable` here is the cable's **series
/// resistance** only; the cable's shunt capacitance is a separate one-pole filter, not
/// part of this resistive solve. Pass `Ohms::ZERO` for `z_cable` when there's no cable.
///
/// The solve stays **linear** by design — nonlinearity (clipping, saturation) lives in a
/// device's transfer function upstream of `v_src`, never in the interconnect.
///
/// # Panics
/// Panics if `Zout + Zcable + Zin == 0` (only possible if all three are zero ohms), which
/// would be a `0/0` gain. Impedances are fixed at setup time, so this is a construction
/// bug caught here rather than a silent `NaN` on the signal path.
#[must_use]
pub fn divider_gain(z_out: Ohms, z_cable: Ohms, load: InputZ) -> f32 {
    let z_in = f64::from(load.z_in().get());
    let denom = f64::from(z_out.get()) + f64::from(z_cable.get()) + z_in;
    assert!(
        denom > 0.0,
        "voltage divider needs Zout + Zcable + Zin > 0, got {denom}"
    );
    (z_in / denom) as f32
}

/// Per-branch open-circuit-to-receiver gains for a fan-out output node.
///
/// A single output drives one or more branches, each a cable series resistance in series with
/// a load `InputZ`, all hanging in parallel off the source's `z_out`. The source sets the node
/// voltage against the **combined** parallel load; each receiver then sees its own branch
/// divider. Returns one gain per branch, in the order given:
///
/// ```text
///   Z_load = ∥_i (R_cable_i + Zin_i)
///   node   = Z_load / (Zout + Z_load)              // source  → node
///   gain_i = node · Zin_i / (R_cable_i + Zin_i)    // node    → receiver i
/// ```
///
/// For a single branch this collapses to exactly [`divider_gain`]. This is the **resistive**
/// solve; each branch's treble rolloff (the cable's shunt-C one-pole) is built separately by
/// [`Cable::lowpass`](super::Cable::lowpass), and under genuine fan-out its corner is the
/// per-branch single-load approximation (documented there) — exact for the no-fan-out chains
/// we run through Epic 2.
///
/// Allocates (compile-time only, never the hot path). Empty `branches` ⇒ empty result.
///
/// # Panics
/// Panics if any branch's `R_cable + Zin == 0`, or if `Zout + Z_load == 0` — only possible
/// with all-zero impedances, a construction bug rather than a silent `NaN` on the path.
#[allow(dead_code, reason = "first consumer is compile (Task 1.3.5)")]
#[must_use]
pub(crate) fn fan_out_gains(z_out: Ohms, branches: &[(Ohms, InputZ)]) -> Vec<f32> {
    if branches.is_empty() {
        return Vec::new();
    }

    // Combined parallel load the source drives: the branches' (R_cable + Zin) in parallel.
    let z_load = branches
        .iter()
        .map(|(r_cable, load)| *r_cable + load.z_in())
        .reduce(Ohms::parallel)
        .expect("branches is non-empty");

    let denom_node = f64::from(z_out.get()) + f64::from(z_load.get());
    assert!(
        denom_node > 0.0,
        "fan-out node needs Zout + Zload > 0, got {denom_node}"
    );
    let node_gain = f64::from(z_load.get()) / denom_node;

    branches
        .iter()
        .map(|(r_cable, load)| {
            let z_in = f64::from(load.z_in().get());
            let branch_denom = f64::from(r_cable.get()) + z_in;
            assert!(
                branch_denom > 0.0,
                "fan-out branch needs R_cable + Zin > 0, got {branch_denom}"
            );
            (node_gain * (z_in / branch_denom)) as f32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::electrical::Thevenin;
    use crate::signal::Volts;
    use approx::assert_relative_eq;

    fn input(ohms: f32) -> InputZ {
        InputZ::new(Ohms::new(ohms))
    }

    #[test]
    fn bridging_is_almost_unity() {
        // Modern pro audio: 100 Ω source into a 10 kΩ input, no cable.
        // gain = 10000 / (100 + 0 + 10000) = 0.990099  →  −0.086 dB. Negligible.
        let g = divider_gain(Ohms::new(100.0), Ohms::ZERO, input(10_000.0));
        assert_relative_eq!(g, 0.990_099, epsilon = 1e-5);
    }

    #[test]
    fn matching_600_ohms_is_minus_6_db() {
        // Vintage 600 Ω matching: Zout = Zin = 600 Ω, no cable.
        // gain = 600 / (600 + 0 + 600) = 0.5 exactly  →  20·log10(0.5) = −6.02 dB.
        let g = divider_gain(Ohms::new(600.0), Ohms::ZERO, input(600.0));
        assert_relative_eq!(g, 0.5, epsilon = 1e-6);
    }

    #[test]
    fn loaded_down_is_the_bridging_mistake() {
        // Bridging backwards: a high-Z 10 kΩ source into a low-Z 600 Ω input.
        // gain = 600 / (10000 + 0 + 600) = 0.056603  →  20·log10(0.056603) = −24.9 dB. Ugly.
        let g = divider_gain(Ohms::new(10_000.0), Ohms::ZERO, input(600.0));
        assert_relative_eq!(g, 0.056_603, epsilon = 1e-5);
        assert!(
            g < 0.06,
            "a low-Z load on a high-Z source should lose a lot of level"
        );
    }

    #[test]
    fn cable_series_resistance_adds_a_little_loss() {
        // 100 Ω source + 50 Ω of cable R into 10 kΩ: gain = 10000 / 10150 = 0.985222.
        let g = divider_gain(Ohms::new(100.0), Ohms::new(50.0), input(10_000.0));
        assert_relative_eq!(g, 0.985_222, epsilon = 1e-5);
    }

    #[test]
    fn fan_out_increases_loss() {
        // Splitting into two 10 kΩ inputs presents them in parallel = 5 kΩ.
        // gain = 5000 / (100 + 0 + 5000) = 0.980392, vs 0.990099 for a single load.
        let single = divider_gain(Ohms::new(100.0), Ohms::ZERO, input(10_000.0));
        let combined = Ohms::new(10_000.0).parallel(Ohms::new(10_000.0));
        let fanned = divider_gain(Ohms::new(100.0), Ohms::ZERO, InputZ::new(combined));
        assert_relative_eq!(fanned, 0.980_392, epsilon = 1e-5);
        assert!(
            fanned < single,
            "fan-out lowers Zin, so it should lose more level"
        );
    }

    #[test]
    fn gain_applies_to_the_source_voltage() {
        // The caller multiplies: a 1.0 V source into a matched load delivers 0.5 V.
        let src = Thevenin::new(Volts::new(1.0), Ohms::new(600.0));
        let g = divider_gain(src.z_out(), Ohms::ZERO, input(600.0));
        let v_in = src.v_src() * g;
        assert_relative_eq!(v_in.get(), 0.5, epsilon = 1e-6);
    }

    #[test]
    #[should_panic(expected = "Zout + Zcable + Zin > 0")]
    fn rejects_all_zero_impedances() {
        let _ = divider_gain(Ohms::ZERO, Ohms::ZERO, InputZ::new(Ohms::ZERO));
    }

    #[test]
    fn single_branch_fan_out_equals_divider_gain() {
        // One branch must collapse to the plain divider: 100 Ω source, 50 Ω cable, 10 kΩ in.
        let g_div = divider_gain(Ohms::new(100.0), Ohms::new(50.0), input(10_000.0));
        let g_fan = fan_out_gains(Ohms::new(100.0), &[(Ohms::new(50.0), input(10_000.0))]);
        assert_eq!(g_fan.len(), 1);
        assert_relative_eq!(g_fan[0], g_div, epsilon = 1e-7);
    }

    #[test]
    fn two_equal_branches_share_a_lower_node_voltage() {
        // 100 Ω source, no cable, into two equal 10 kΩ loads.
        // Z_load = 10k ∥ 10k = 5k; node = 5000/5100 = 0.980392; each branch divider = 1.
        let g = fan_out_gains(
            Ohms::new(100.0),
            &[(Ohms::ZERO, input(10_000.0)), (Ohms::ZERO, input(10_000.0))],
        );
        assert_eq!(g.len(), 2);
        assert_relative_eq!(g[0], 0.980_392, epsilon = 1e-5);
        assert_relative_eq!(g[1], 0.980_392, epsilon = 1e-5);
        // Loading down to the parallel 5 kΩ loses more than a single 10 kΩ load (0.990099).
        let single = divider_gain(Ohms::new(100.0), Ohms::ZERO, input(10_000.0));
        assert!(g[0] < single);
    }

    #[test]
    fn unequal_branches_divide_off_a_shared_node() {
        // 100 Ω source into a 10 kΩ load and a 1 kΩ load (no cables).
        // Z_load = 10k ∥ 1k = 909.0909 Ω; node = 909.0909 / 1009.0909 = 0.900901.
        // Each branch divider is unity (no cable), so both see the node; the heavier (1 kΩ)
        // load is what drags the shared node down.
        let g = fan_out_gains(
            Ohms::new(100.0),
            &[(Ohms::ZERO, input(10_000.0)), (Ohms::ZERO, input(1_000.0))],
        );
        assert_relative_eq!(g[0], 0.900_901, epsilon = 1e-5);
        assert_relative_eq!(g[1], 0.900_901, epsilon = 1e-5);
    }

    #[test]
    fn cable_on_one_branch_attenuates_only_that_branch_further() {
        // Source 100 Ω; branch A: 10 kΩ via 1 kΩ of cable R; branch B: 10 kΩ direct.
        // Z_load = (1000+10000) ∥ 10000 = 11000·10000/21000 = 5238.095 Ω.
        // node = 5238.095 / 5338.095 = 0.981268.
        // A divider = 10000/11000 = 0.909091 → gain_A = 0.981268·0.909091 = 0.892062.
        // B divider = 1.0               → gain_B = 0.981268.
        let g = fan_out_gains(
            Ohms::new(100.0),
            &[
                (Ohms::new(1_000.0), input(10_000.0)),
                (Ohms::ZERO, input(10_000.0)),
            ],
        );
        assert_relative_eq!(g[0], 0.892_062, epsilon = 1e-5);
        assert_relative_eq!(g[1], 0.981_268, epsilon = 1e-5);
        assert!(g[0] < g[1], "the cabled branch should lose a little more");
    }

    #[test]
    fn empty_branches_is_empty() {
        assert!(fan_out_gains(Ohms::new(100.0), &[]).is_empty());
    }
}
