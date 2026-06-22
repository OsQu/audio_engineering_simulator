//! Level conversions between linear volts and decibel measurement scales.
//!
//! Decibels are *measurements*, not storage — these helpers are the only place a voltage
//! (or a normalized digital sample) becomes dB (and back).
//!
//! - **dBu** — reference 0 dBu = √0.6 V ≈ 0.7746 V (the RMS voltage that delivers 1 mW
//!   into 600 Ω). Commonly quoted, rounded, as 0.775 V.
//! - **dBV** — reference 0 dBV = 1 V.
//! - **dBFS** — full-scale digital level: [`sample_to_dbfs`] reads a normalized
//!   [`SampleBuffer`](crate::SampleBuffer) value (±1.0 = full scale) as `20·log10(|sample|)`. The
//!   volts↔dBFS *calibration* (how a real voltage maps to full scale) is owned by the AD converter
//!   via its reference voltage (Story 1.6); this is the pure sample-domain reading.
//!
//! Inputs are *levels* (a magnitude/RMS voltage, expected ≥ 0). A level of 0 V converts
//! to −∞ dB, as the math dictates. Computation is done in `f64` so round-trips stay tight.

use crate::signal::Volts;

/// 0 dBu reference voltage: √0.6 V ≈ 0.7746 V.
const V_REF_DBU: f64 = 0.774_596_669_241_483_4;

/// 0 dBV reference voltage: 1 V.
const V_REF_DBV: f64 = 1.0;

/// Convert a level in dBu to volts.
#[must_use]
pub fn dbu_to_volts(dbu: f32) -> Volts {
    Volts::new(db_to_volts(f64::from(dbu), V_REF_DBU))
}

/// Convert a level in volts to dBu. A level of 0 V maps to −∞ dB.
#[must_use]
pub fn volts_to_dbu(v: Volts) -> f32 {
    volts_to_db(v, V_REF_DBU)
}

/// Convert a level in dBV to volts.
#[must_use]
pub fn dbv_to_volts(dbv: f32) -> Volts {
    Volts::new(db_to_volts(f64::from(dbv), V_REF_DBV))
}

/// Convert a level in volts to dBV. A level of 0 V maps to −∞ dB.
#[must_use]
pub fn volts_to_dbv(v: Volts) -> f32 {
    volts_to_db(v, V_REF_DBV)
}

/// Headroom in dB: how far a signal `peak` sits below the clip point `rail`,
/// `20·log10(rail / peak)`.
///
/// This is the dB "room" left before the rail clamps the waveform — large for a quiet signal,
/// 0 dB right at the onset of clipping, negative once the signal is driven past the rail. A
/// `peak` of 0 V (silence) has infinite headroom (+∞). It's a *ratio* of two levels, so the
/// reference cancels and the answer is independent of dBu/dBV.
#[must_use]
pub fn headroom_db(peak: Volts, rail: Volts) -> f32 {
    (20.0 * (f64::from(rail.get()) / f64::from(peak.get())).log10()) as f32
}

/// dBFS of a normalized digital sample, where ±1.0 is full scale: `20·log10(|sample|)`.
///
/// A *measurement* of a [`SampleBuffer`](crate::SampleBuffer) value (linear, normalized). A sample
/// of 0 maps to −∞ dBFS. The volts↔dBFS calibration lives on the AD (its reference voltage).
#[must_use]
pub fn sample_to_dbfs(sample: f32) -> f32 {
    (20.0 * f64::from(sample.abs()).log10()) as f32
}

// Shared math, in f64 for precision. `db = 20·log10(V / V_ref)`, inverted as
// `V = V_ref · 10^(db / 20)`.

fn db_to_volts(db: f64, v_ref: f64) -> f32 {
    (v_ref * 10.0_f64.powf(db / 20.0)) as f32
}

fn volts_to_db(v: Volts, v_ref: f64) -> f32 {
    (20.0 * (f64::from(v.get()) / v_ref).log10()) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn zero_dbu_is_reference_voltage() {
        // 0 dBu = √0.6 V ≈ 0.7746 V (often rounded to 0.775).
        assert_relative_eq!(dbu_to_volts(0.0).get(), 0.774_596_7, epsilon = 1e-5);
    }

    #[test]
    fn plus_4_dbu_is_about_1_23_volts() {
        // +4 dBu = √0.6 · 10^(4/20) = 0.774_597 · 1.584_893 ≈ 1.2283 V.
        assert_relative_eq!(dbu_to_volts(4.0).get(), 1.2283, epsilon = 1e-3);
    }

    #[test]
    fn zero_dbv_is_one_volt() {
        assert_relative_eq!(dbv_to_volts(0.0).get(), 1.0, epsilon = 1e-6);
    }

    #[test]
    fn minus_10_dbv_is_about_0_316_volts() {
        // −10 dBV = 10^(−10/20) = 10^(−0.5) ≈ 0.31623 V.
        assert_relative_eq!(dbv_to_volts(-10.0).get(), 0.316_227_77, epsilon = 1e-5);
    }

    #[test]
    fn dbu_round_trips() {
        for &dbu in &[-20.0_f32, -6.0, 0.0, 4.0, 24.0] {
            assert_relative_eq!(volts_to_dbu(dbu_to_volts(dbu)), dbu, epsilon = 1e-3);
        }
    }

    #[test]
    fn dbv_round_trips() {
        for &dbv in &[-20.0_f32, -10.0, 0.0, 12.0] {
            assert_relative_eq!(volts_to_dbv(dbv_to_volts(dbv)), dbv, epsilon = 1e-3);
        }
    }

    #[test]
    fn headroom_is_the_db_below_the_rail() {
        // A 1 V peak under a 10 V rail: 20·log10(10/1) = 20 dB of headroom.
        assert_relative_eq!(
            headroom_db(Volts::new(1.0), Volts::new(10.0)),
            20.0,
            epsilon = 1e-4
        );
        // Right at the rail there is no headroom left: 20·log10(1) = 0 dB.
        assert_relative_eq!(
            headroom_db(Volts::new(10.0), Volts::new(10.0)),
            0.0,
            epsilon = 1e-6
        );
        // Driven past the rail, headroom goes negative (the overdrive in dB): 20·log10(10/20)
        // = −6.02 dB.
        assert_relative_eq!(
            headroom_db(Volts::new(20.0), Volts::new(10.0)),
            -6.0206,
            epsilon = 1e-3
        );
    }

    #[test]
    fn sample_to_dbfs_reads_full_scale_and_halves() {
        // ±1.0 is 0 dBFS; half-scale (0.5) is 20·log10(0.5) = −6.02 dBFS.
        assert_relative_eq!(sample_to_dbfs(1.0), 0.0, epsilon = 1e-6);
        assert_relative_eq!(sample_to_dbfs(-1.0), 0.0, epsilon = 1e-6);
        assert_relative_eq!(sample_to_dbfs(0.5), -6.0206, epsilon = 1e-3);
        // +4 dBu (1.737 V peak) into a 13.80 V-peak full-scale converter reads −18 dBFS.
        assert_relative_eq!(sample_to_dbfs(1.7372 / 13.80), -18.0, epsilon = 0.05);
    }

    #[test]
    fn dbu_reads_about_2_2_db_hotter_than_dbv() {
        // The same voltage reads ~2.218 dB higher on dBu than dBV, because
        // 0 dBu (0.7746 V) sits below 0 dBV (1 V): 20·log10(1 / 0.7746) ≈ 2.218 dB.
        let v = Volts::new(1.0);
        assert_relative_eq!(volts_to_dbu(v) - volts_to_dbv(v), 2.218, epsilon = 1e-2);
    }
}
