//! The implicit capture: the harness-side analog→digital conversion that turns the simulation's
//! tapped speaker voltage into host-rate samples (for a WAV now, real-time playback later).
//!
//! It sits **outside** the engine on purpose. The simulation ends in the analog domain at the
//! speaker's output; this captures that voltage and resamples it to the host rate. It is the
//! PROJECT_PLAN §5.1 "internal AD" role minus the acoustics and minus node status — it carries
//! **no clock domain**, rides on **no modeled-converter rate**, and has no dBFS-calibration
//! meaning. Its one job is to stay **transparent**: every audible artifact in a render must
//! originate in the *modeled* AD/DA under test, never here. So it reuses the same windowed-sinc
//! [`Decimator`] the modeled AD uses, with the same steep default spec.

use engine::{AnalogRate, Decimator, SampleRate, kaiser_beta};

/// Anti-alias filter length / stopband — mirrors the modeled AD's transparent default
/// (`AdConverter`'s `DEFAULT_AA_TAPS` / `DEFAULT_STOPBAND_DB`), so the capture is at least as
/// clean as the converters it renders.
const AA_TAPS: usize = 161;
const STOPBAND_DB: f64 = 96.0;

/// Converts the tapped speaker **voltage** (at the analog rate) into normalized host **samples**
/// (±1.0 = full scale, at the host rate).
///
/// Stateful: hold **one** instance for a whole render and feed it block by block — the decimator's
/// tap history carries across blocks, so re-creating it per block would inject discontinuities.
/// Volts map to full scale through a **fixed monitor reference** (no per-render auto-normalization,
/// which would break determinism and cross-render level comparison).
pub struct Capture {
    decimator: Decimator,
    /// `1 / full_scale_volts`: the fixed monitor reference (speaker volts mapping to ±1.0).
    inv_full_scale: f32,
}

impl Capture {
    /// A capture decimating `analog_rate` → `host_rate`, mapping `full_scale_volts` of speaker
    /// voltage to digital full scale (±1.0).
    ///
    /// # Panics
    /// Panics unless `host_rate` integer-divides `analog_rate` (Epic 2 keeps the host rate an
    /// integer divisor of the analog rate — a fractional resampler is deferred), and unless
    /// `full_scale_volts` is finite and `> 0`. This is render tooling, off the engine's hot path,
    /// so it validates loudly at construction.
    #[must_use]
    pub fn new(analog_rate: AnalogRate, host_rate: SampleRate, full_scale_volts: f32) -> Self {
        let ratio = analog_rate.as_hz() / host_rate.as_hz();
        let m = ratio.round();
        assert!(
            m >= 1.0 && (ratio - m).abs() < 1e-9,
            "capture host rate ({}) must integer-divide the analog rate ({})",
            host_rate.as_hz(),
            analog_rate.as_hz(),
        );
        assert!(
            full_scale_volts.is_finite() && full_scale_volts > 0.0,
            "capture full-scale volts must be finite and > 0, got {full_scale_volts}"
        );
        Self {
            decimator: Decimator::lowpass(AA_TAPS, m as usize, kaiser_beta(STOPBAND_DB)),
            inv_full_scale: 1.0 / full_scale_volts,
        }
    }

    /// The decimation factor `M`: analog samples consumed per host sample produced.
    #[must_use]
    pub fn factor(&self) -> usize {
        self.decimator.factor()
    }

    /// Host samples produced from `analog_len` analog-rate input samples.
    #[must_use]
    pub fn host_len(&self, analog_len: usize) -> usize {
        analog_len / self.factor()
    }

    /// Decimate one block of speaker **volts** into host **samples**, scaled by the monitor
    /// reference and clamped to ±1.0.
    ///
    /// # Panics
    /// Panics unless `volts.len() == out.len() * factor()`.
    pub fn process(&mut self, volts: &[f32], out: &mut [f32]) {
        assert_eq!(
            volts.len(),
            out.len() * self.factor(),
            "capture input length must be output length × factor",
        );
        // Decimate in volts (the FIR is linear, so the reference scaling commutes — apply it after,
        // at the cheaper host rate).
        self.decimator.process(volts, out);
        let inv = self.inv_full_scale;
        for s in out.iter_mut() {
            *s = (*s * inv).clamp(-1.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    const FULL_SCALE: f32 = 5.0;

    fn capture() -> Capture {
        Capture::new(
            AnalogRate::new(384_000.0),
            SampleRate::new(48_000.0),
            FULL_SCALE,
        )
    }

    /// Steady DC settles to `volts / full_scale` once the 161-tap history has filled; check the
    /// last output of a 384-sample block (48 outputs — well past the ~20-output settling point).
    fn settled_dc(volts: f32) -> f32 {
        let mut cap = capture();
        let input = vec![volts; 384];
        let mut out = vec![0.0_f32; cap.host_len(384)];
        cap.process(&input, &mut out);
        out[out.len() - 1]
    }

    #[test]
    fn factor_and_host_len() {
        let cap = capture();
        assert_eq!(cap.factor(), 8); // 384 kHz / 48 kHz
        assert_eq!(cap.host_len(384), 48);
    }

    #[test]
    fn full_scale_volts_map_to_unity() {
        // Unity-DC-gain decimator: full-scale volts → ±1.0 sample.
        assert_relative_eq!(settled_dc(FULL_SCALE), 1.0, epsilon = 1e-3);
    }

    #[test]
    fn scales_by_the_monitor_reference() {
        // Half the reference voltage → half full scale.
        assert_relative_eq!(settled_dc(0.5 * FULL_SCALE), 0.5, epsilon = 1e-3);
    }

    #[test]
    fn clamps_a_digital_over() {
        // Twice the reference would read 2.0 — a digital "over" clamps hard at full scale.
        assert_relative_eq!(settled_dc(2.0 * FULL_SCALE), 1.0, epsilon = 1e-6);
    }
}
