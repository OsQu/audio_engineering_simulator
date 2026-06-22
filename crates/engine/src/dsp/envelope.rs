//! A peak envelope follower — the level detector behind dynamics processing.

use super::flush_denormal;
use crate::signal::SampleRate;

/// A **peak** envelope follower: rectify the input, then track it with a one-pole that **rises**
/// with the attack coefficient and **falls** with the release coefficient.
///
/// The amplitude-domain cousin of [`OnePole`](crate::OnePole): the same `env += a·(x − env)`
/// recurrence, but with `a` switched per sample by whether the rectified input is above the current
/// envelope (attack, catching a transient) or below it (release, letting go). Coefficients come from
/// time constants: `a = 1 − e^(−1/(τ·fs))`, so the envelope reaches `1 − 1/e ≈ 63.2 %` of a step in
/// `τ` seconds (a `τ` of 0 ⇒ `a = 1`, instantaneous). Designed once at construction; `step` is the
/// hot path — denormal-flushed, panic-free.
#[derive(Debug, Clone)]
pub struct PeakEnvelope {
    attack: f64,
    release: f64,
    env: f64,
}

impl PeakEnvelope {
    /// A follower at `rate` with `attack_ms` / `release_ms` time constants (the time to reach
    /// ~63 % of a step). A time of 0 ms gives an instantaneous coefficient (`a = 1`).
    #[must_use]
    pub fn new(rate: SampleRate, attack_ms: f64, release_ms: f64) -> Self {
        Self {
            attack: coeff(rate, attack_ms),
            release: coeff(rate, release_ms),
            env: 0.0,
        }
    }

    /// Clear the envelope to silence. Off the hot path.
    pub fn reset(&mut self) {
        self.env = 0.0;
    }

    /// Feed one sample; return the updated envelope (a non-negative amplitude). Hot path:
    /// `#[inline]`, one branch (attack vs. release), denormal-flushed.
    #[inline]
    pub(crate) fn step(&mut self, x: f64) -> f64 {
        let rect = x.abs();
        let a = if rect > self.env {
            self.attack
        } else {
            self.release
        };
        self.env = flush_denormal(self.env + a * (rect - self.env));
        self.env
    }
}

/// The one-pole coefficient `a = 1 − e^(−1/(τ·fs))` for a time constant of `ms` milliseconds at
/// `rate`; `ms ≤ 0` ⇒ `a = 1` (instantaneous, no smoothing).
fn coeff(rate: SampleRate, ms: f64) -> f64 {
    let n = ms * 1e-3 * rate.as_hz();
    if n <= 0.0 {
        1.0
    } else {
        1.0 - (-1.0 / n).exp()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    fn fs() -> SampleRate {
        SampleRate::new(48_000.0)
    }

    #[test]
    fn attack_reaches_63_percent_in_one_time_constant() {
        // 10 ms attack at 48 kHz = 480 samples. After feeding a constant 1.0 for 480 samples, a
        // one-pole step response sits at 1 − 1/e ≈ 0.632.
        let mut env = PeakEnvelope::new(fs(), 10.0, 100.0);
        let mut last = 0.0;
        for _ in 0..480 {
            last = env.step(1.0);
        }
        assert_relative_eq!(last, 1.0 - (-1.0_f64).exp(), epsilon = 1e-3);
    }

    #[test]
    fn release_falls_to_37_percent_in_one_time_constant() {
        // Charge to ~1.0 with a fast attack, then release: 10 ms release = 480 samples to fall to
        // 1/e ≈ 0.368 of the starting value.
        let mut env = PeakEnvelope::new(fs(), 0.0, 10.0);
        env.step(1.0); // instantaneous attack ⇒ env = 1.0
        assert_relative_eq!(env.step(1.0), 1.0, epsilon = 1e-9);
        let mut last = 0.0;
        for _ in 0..480 {
            last = env.step(0.0); // input below env ⇒ release
        }
        assert_relative_eq!(last, (-1.0_f64).exp(), epsilon = 1e-3);
    }

    #[test]
    fn tracks_the_peak_not_the_instantaneous_value() {
        // With a fast attack and a slow release, the envelope holds near the peak between peaks
        // rather than following the signal back down to zero.
        let mut env = PeakEnvelope::new(fs(), 0.0, 1_000.0);
        env.step(0.8); // jump to the peak
        let after_dip = env.step(0.0); // a sudden zero shouldn't collapse the envelope
        assert!(
            after_dip > 0.79,
            "slow release should hold near the peak, got {after_dip}"
        );
    }

    #[test]
    fn reset_clears_the_envelope() {
        let mut env = PeakEnvelope::new(fs(), 0.0, 1_000.0);
        env.step(0.9); // charge it up
        env.reset();
        // After reset the envelope is silent again, so a fresh step starts from zero.
        assert_relative_eq!(env.step(0.0), 0.0, epsilon = 1e-9);
    }

    #[test]
    fn zero_times_are_instantaneous() {
        let mut env = PeakEnvelope::new(fs(), 0.0, 0.0);
        assert_relative_eq!(env.step(0.5), 0.5, epsilon = 1e-9); // tracks immediately
        assert_relative_eq!(env.step(0.0), 0.0, epsilon = 1e-9); // and drops immediately
    }
}
