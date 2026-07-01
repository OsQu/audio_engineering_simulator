//! The cable catalog: named cable *types* the UI offers when patching two analog jacks.
//!
//! A cable is **physical content** — its series resistance and shunt capacitance are as intrinsic as a
//! device's impedance — so, like device dimensions (Story 4.3), it lives here on the content layer, not
//! invented in the TypeScript UI where it would drift. Each [`CableType`] maps a stable `type_id` to a
//! realistic R·C (authored from a per-metre basis at a nominal length) plus a connector `kind` for jack
//! styling. The UI fetches the table, the user picks one when wiring an **analog↔analog** connection,
//! and the chosen R·C rides the edge as the scene [`CableSpec`](crate::CableSpec) — where the engine's
//! loading divider + one-pole treble rolloff emerge from it (Epic 1.2).
//!
//! **Modelled, not necessarily audible — by design.** The rolloff corner is
//! `f_c = 1/(2π · R_thev · C)` with `R_thev = (Zout + R_cable) ∥ Zin` (see `engine`'s `Cable::lowpass`),
//! dominated by the *smaller* of source and load impedance. Every source in today's catalog is low-Z
//! (synth 1 Ω, gain/DA 150 Ω), so a realistic cable's corner sits far above 20 kHz and the series-R
//! level drop is negligible: a clean chain does not degrade audibly even though the physics is faithful.
//! That is the point, and it matches the project's rule that cable loss is a **numeric** oracle, not an
//! ear test. The audible payoff arrives with high-Z instrument sources in Epic 5 (and becomes *visible*
//! via the analog-domain readouts in Story 4.5).
//!
//! Digital / event routes carry no cable (the engine ignores a `CableSpec` on a non-analog edge), so the
//! UI offers cables on analog connections only.

use crate::catalog::PortKind;
use serde::Serialize;

/// One cable type the UI can offer — a realistic R·C preset plus a connector kind. Numeric fields are SI
/// (ohms, farads); `length_m` is informational (the nominal length the R·C was authored at, and the seam
/// for future length-scaling). Field names are camelCase on the JS side, matching the `CableSpec` wire
/// shape the engine ingress already uses.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CableType {
    /// Stable catalog id — what a UI cable picker selects (and could persist alongside the `CableSpec`).
    pub type_id: String,
    /// Human display name (e.g. "Instrument Cable (6 m)").
    pub label: String,
    /// Connector kind, for jack/cable styling (mirrors a port's `kind`).
    pub kind: PortKind,
    /// Nominal length in metres the R·C was authored at (display + length-scaling seam).
    pub length_m: f32,
    /// Series resistance, ohms (the `Zcable` term of the loading divider).
    pub resistance_ohms: f32,
    /// Shunt capacitance, farads (forms the treble-rolloff one-pole with the resistance it sees).
    pub capacitance_farads: f32,
}

/// One cable preset in the table (static, `&'static str` label), expanded to a [`CableType`] by
/// [`cable_types`]. R·C authored from a per-metre basis (audio cable is *electrically short*, so a lumped
/// R·C is exact enough — see `engine`'s `Cable`): instrument/patch coax ≈ 100 pF/m, balanced mic ≈
/// 50 pF/m, and a small series resistance for the conductor run.
struct CableEntry {
    type_id: &'static str,
    label: &'static str,
    kind: PortKind,
    length_m: f32,
    resistance_ohms: f32,
    capacitance_farads: f32,
}

/// The cable catalog: the realistic presets the UI offers for analog connections. Ordered shortest/least
/// lossy first, so a picker's default (index 0) is the most transparent choice.
const CABLES: &[CableEntry] = &[
    // A short studio patch cable — near-ideal (the sensible default when wiring).
    CableEntry {
        type_id: "patch_short",
        label: "Patch Cable (0.5 m)",
        kind: PortKind::Line,
        length_m: 0.5,
        resistance_ohms: 0.05,
        capacitance_farads: 5.0e-11, // 0.5 m × ~100 pF/m = 50 pF
    },
    // A typical instrument/line cable.
    CableEntry {
        type_id: "instrument_3m",
        label: "Instrument Cable (3 m)",
        kind: PortKind::Instrument,
        length_m: 3.0,
        resistance_ohms: 0.15,
        capacitance_farads: 3.0e-10, // 3 m × ~100 pF/m = 300 pF
    },
    // A long instrument cable — the classic "darker" cable (more pF ⇒ lower corner).
    CableEntry {
        type_id: "instrument_6m",
        label: "Instrument Cable (6 m)",
        kind: PortKind::Instrument,
        length_m: 6.0,
        resistance_ohms: 0.3,
        capacitance_farads: 6.0e-10, // 6 m × ~100 pF/m = 600 pF
    },
    // A balanced mic cable (lower pF/m than coax).
    CableEntry {
        type_id: "mic_10m",
        label: "Mic Cable (10 m)",
        kind: PortKind::Mic,
        length_m: 10.0,
        resistance_ohms: 0.5,
        capacitance_farads: 5.0e-10, // 10 m × ~50 pF/m = 500 pF
    },
    // A speaker cable — low resistance run; capacitance is minor (we don't model inductance).
    CableEntry {
        type_id: "speaker_5m",
        label: "Speaker Cable (5 m)",
        kind: PortKind::Speaker,
        length_m: 5.0,
        resistance_ohms: 0.1,
        capacitance_farads: 2.5e-10,
    },
];

/// The full cable catalog, one [`CableType`] per preset — what the UI fetches to populate the cable
/// picker. Cold path (UI startup). Mirrors [`descriptors`](crate::descriptors) for the device catalog.
#[must_use]
pub fn cable_types() -> Vec<CableType> {
    CABLES
        .iter()
        .map(|c| CableType {
            type_id: c.type_id.to_owned(),
            label: c.label.to_owned(),
            kind: c.kind,
            length_m: c.length_m,
            resistance_ohms: c.resistance_ohms,
            capacitance_farads: c.capacitance_farads,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::{AnalogRate, Cable, Farads, InputZ, Ohms, OnePole, VoltageBuffer, divider_gain};

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    /// The preset with the given `type_id` (panics in tests if missing).
    fn preset(type_id: &str) -> CableType {
        cable_types()
            .into_iter()
            .find(|c| c.type_id == type_id)
            .unwrap_or_else(|| panic!("no cable preset {type_id}"))
    }

    fn cable_of(c: &CableType) -> Cable {
        Cable::new(
            Ohms::new(c.resistance_ohms),
            Farads::new(c.capacitance_farads),
        )
    }

    /// Steady-state gain of a one-pole at `freq` (RMS out / RMS in over the settled second half of a
    /// long sine). A minimal local version of the engine's `measure_gain`, so this oracle exercises the
    /// real filter rather than only re-deriving the corner arithmetic.
    fn measure_gain(mut filter: OnePole, freq: f64, rate: AnalogRate) -> f64 {
        let n = 16_384usize;
        let fs = rate.as_hz();
        let mut buf = VoltageBuffer::zeros(n, rate);
        for (i, s) in buf.as_mut_slice().iter_mut().enumerate() {
            *s = (core::f64::consts::TAU * freq * i as f64 / fs).sin() as f32;
        }
        filter.process(&mut buf);
        // RMS over the settled second half (the one-pole reaches steady state in a handful of samples).
        let half = n / 2;
        let out_rms = {
            let tail = &buf.as_slice()[half..];
            (tail
                .iter()
                .map(|&v| f64::from(v) * f64::from(v))
                .sum::<f64>()
                / tail.len() as f64)
                .sqrt()
        };
        // Input is a unit sine ⇒ RMS = 1/√2.
        out_rms / (1.0 / core::f64::consts::SQRT_2)
    }

    /// **Hand-calc oracle** — a specific preset's R·C, driven by a *representative high-Z source*,
    /// produces the modelled treble rolloff at the frequency the formula predicts.
    ///
    /// `instrument_6m`: R = 0.3 Ω, C = 600 pF. Into a passive-instrument-like source
    /// `Zout = 10 kΩ` and a high-Z pedal/DI input `Zin = 1 MΩ`:
    ///   R_thev = (10000 + 0.3) ∥ 1e6 = 10000.3·1e6 / (1e6 + 10000.3) = 9900.99 Ω
    ///   f_c    = 1 / (2π · 9900.99 · 600e-12) = 1 / (2π · 5.9406e-6) = 26_792 Hz
    /// A one-pole is −3 dB (gain 1/√2 ≈ 0.7071) at f_c and ~unity a decade below.
    #[test]
    fn instrument_cable_rolloff_matches_hand_calc() {
        let c = preset("instrument_6m");
        let z_out = Ohms::new(10_000.0);
        let load = InputZ::new(Ohms::new(1_000_000.0));
        let f_c = 26_792.0;

        let g_corner = measure_gain(cable_of(&c).lowpass(z_out, load, rate()), f_c, rate());
        assert!(
            (g_corner - core::f64::consts::FRAC_1_SQRT_2).abs() < 1.5e-2,
            "at the hand-calc corner {f_c} Hz the gain should be ≈0.707, got {g_corner}"
        );

        let g_below = measure_gain(
            cable_of(&c).lowpass(z_out, load, rate()),
            f_c / 10.0,
            rate(),
        );
        assert!(
            g_below > 0.98,
            "a decade below f_c the cable should pass ~unity, got {g_below}"
        );
    }

    /// **Modelled-but-inaudible, by design** — the *same* preset into the catalog's real low-Z synth
    /// source (`Zout = 1 Ω`) puts the corner far above the audio band, so a clean chain doesn't degrade
    /// audibly even though the R·C is faithfully modelled (§9: cable loss is a numeric oracle).
    ///
    /// synth `Zout = 1 Ω`, AD input `Zin = 1 MΩ`, `instrument_6m` (R = 0.3 Ω, C = 600 pF):
    ///   R_thev = (1 + 0.3) ∥ 1e6 ≈ 1.3 Ω
    ///   f_c    = 1 / (2π · 1.3 · 600e-12) ≈ 2.04e8 Hz  (204 MHz — utterly inaudible)
    #[test]
    fn realistic_cable_into_low_z_source_is_inaudible() {
        let c = preset("instrument_6m");
        let r_cable = Ohms::new(c.resistance_ohms);
        let z_out = Ohms::new(1.0); // the synth's real output impedance
        let load = InputZ::new(Ohms::new(1_000_000.0)); // the AD's real input impedance

        let r_thev = (z_out + r_cable).parallel(load.z_in());
        let f_c = 1.0
            / (core::f64::consts::TAU * f64::from(r_thev.get()) * f64::from(c.capacitance_farads));
        assert!(
            f_c > 1.0e6,
            "into a low-Z source the corner must sit far above audio (got {f_c} Hz)"
        );

        // And the audio-band gain is effectively unity — no audible rolloff.
        let g_20k = measure_gain(cable_of(&c).lowpass(z_out, load, rate()), 20_000.0, rate());
        assert!(g_20k > 0.999, "no audible rolloff at 20 kHz, got {g_20k}");
    }

    /// The divider (level) loss is a compile-time scalar `Zin / (Zout + R_cable + Zin)`; with realistic
    /// cable R (< 1 Ω) into a 10 kΩ load it is negligible. Hand calc for `instrument_6m` (R = 0.3):
    ///   10000 / (150 + 0.3 + 10000) = 10000 / 10150.3 = 0.98519   (Zout = 150 Ω line source)
    #[test]
    fn divider_loss_is_negligible_for_realistic_cable() {
        let c = preset("instrument_6m");
        let g = divider_gain(
            Ohms::new(150.0),
            Ohms::new(c.resistance_ohms),
            InputZ::new(Ohms::new(10_000.0)),
        );
        assert!((g - 0.985_19).abs() < 1e-4, "expected ~0.98519, got {g}");
    }

    /// Every preset carries sane physical values — a positive length and R·C in the realistic range
    /// (no zero/negative, and capacitance small enough to be a real cable, not a filter cap).
    #[test]
    fn presets_have_sane_values() {
        for c in cable_types() {
            assert!(c.length_m > 0.0, "{} length", c.type_id);
            assert!(c.resistance_ohms >= 0.0, "{} resistance", c.type_id);
            assert!(
                c.capacitance_farads > 0.0 && c.capacitance_farads < 1.0e-8,
                "{} capacitance {} out of realistic range",
                c.type_id,
                c.capacitance_farads
            );
        }
    }

    /// The catalog serializes in the camelCase wire shape the TS `CableType` mirror consumes (matching
    /// the `CableSpec` field names the engine ingress already uses).
    #[test]
    fn cable_catalog_serializes_camel_case() {
        let json = serde_json::to_string(&cable_types()).expect("cable types serialize");
        assert!(json.contains("typeId"));
        assert!(json.contains("resistanceOhms"));
        assert!(json.contains("capacitanceFarads"));
        assert!(json.contains("lengthM"));
        assert!(json.contains("instrument_6m"), "expected the 6 m preset");
    }
}
