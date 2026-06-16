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
}
