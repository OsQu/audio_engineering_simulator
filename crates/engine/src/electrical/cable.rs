//! The electrical cable: series resistance + shunt capacitance, and its one-pole low-pass.

use super::{Farads, InputZ, Ohms};
use crate::dsp::flush_denormal;
use crate::noise::NoiseDensity;
use crate::signal::{AnalogRate, VoltageBuffer, Volts};

/// An electrical cable: series resistance `r` + shunt capacitance `c`, optionally picking up
/// interference.
///
/// One lumped R-C section, which is exact enough because audio cables are *electrically
/// short* (a 20 kHz wavelength in cable is ~10 km, so metres of cable behave as a lump, not
/// a transmission line). The parts play distinct roles and stay separable:
/// - the **series R** adds to the resistive divider (`Zcable` in [`divider_gain`](super::divider_gain));
/// - the **shunt C**, with the resistance it sees, forms a one-pole low-pass — the treble
///   rolloff ([`Cable::lowpass`]);
/// - the optional **pickup** ([`with_pickup`](Self::with_pickup)) is broadband interference (EMI)
///   coupling *onto* the wire as a noise voltage. It couples **common-mode** — equally onto every
///   conductor — so on a balanced pair it cancels at the receiver difference (Story 1.5.2). The
///   schedule gives the edge its own seeded stream and adds the *same* per-sample draw to each
///   conductor.
/// - the optional **hum** ([`with_hum`](Self::with_hum)) is a 50/60 Hz ground-loop tone, also
///   **common-mode** — the same sine on every conductor — so balanced rejects it and unbalanced
///   carries it (Story 1.5.5). Its phase is seeded from the edge stream for determinism.
///
/// `r` and `c` describe the cable's **differential** path; on a balanced edge the schedule
/// applies the same divider gain and an independent one-pole to each conductor (Story 1.5).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cable {
    r: Ohms,
    c: Farads,
    pickup: NoiseDensity,
    /// Ground-loop hum as `(frequency Hz, amplitude)`, or `None`.
    hum: Option<(f64, Volts)>,
}

impl Cable {
    /// A cable with series resistance `r` and shunt capacitance `c`, and no pickup or hum.
    #[must_use]
    pub fn new(r: Ohms, c: Farads) -> Self {
        Self {
            r,
            c,
            pickup: NoiseDensity::ZERO,
            hum: None,
        }
    }

    /// Set the cable's interference pickup as a spectral density (the same V/√Hz units as a
    /// device noise floor). Builder style: `Cable::new(r, c).with_pickup(density)`.
    #[must_use]
    pub fn with_pickup(mut self, density: NoiseDensity) -> Self {
        self.pickup = density;
        self
    }

    /// Set a ground-loop hum of `amplitude` at `freq_hz` (50 Hz in the EU, 60 Hz in the US),
    /// coupled common-mode. Builder style: `Cable::new(r, c).with_hum(60.0, Volts::new(0.1))`.
    ///
    /// This is a **manual** injection: you are asserting a ground loop exists on this cable. Whether
    /// a loop *actually* exists is a property of the patch's grounding topology (two mains-earthed
    /// devices bonded by a shield), and is intended to become **emergent** from a compile-time
    /// ground-cycle-detection pass — a ground lift or a floating device would then remove the hum on
    /// its own. The amplitude stays phenomenological either way. See `IMPLEMENTATION_PLAN.md`, the
    /// Epic 5 ground-topology decision.
    #[must_use]
    pub fn with_hum(mut self, freq_hz: f64, amplitude: Volts) -> Self {
        self.hum = Some((freq_hz, amplitude));
        self
    }

    /// The cable's series resistance (the `Zcable` term of the divider).
    pub fn r(self) -> Ohms {
        self.r
    }

    /// The cable's shunt capacitance.
    pub fn c(self) -> Farads {
        self.c
    }

    /// The cable's interference pickup density ([`NoiseDensity::ZERO`] if none).
    pub fn pickup(self) -> NoiseDensity {
        self.pickup
    }

    /// The cable's ground-loop hum as `(frequency Hz, amplitude)`, or `None`.
    pub fn hum(self) -> Option<(f64, Volts)> {
        self.hum
    }

    /// Build this cable's one-pole low-pass for a given source and load.
    ///
    /// The shunt cap charges through the Thévenin resistance seen at the input node — the
    /// source and cable resistance in series, in parallel with the load:
    /// `R_thev = (Zout + R_cable) ∥ Zin` — so the corner is `f_c = 1 / (2π · R_thev · C)`.
    #[must_use]
    pub fn lowpass(self, z_out: Ohms, load: InputZ, rate: AnalogRate) -> OnePole {
        let r_thev = (z_out + self.r).parallel(load.z_in());
        OnePole::new(r_thev, self.c, rate)
    }
}

/// A one-pole low-pass filter over voltage, processing a block in place.
///
/// Discretised with the **matched (exact) one-pole** coefficient `a = 1 − e^(−dt/RC)`,
/// `dt = 1/rate`. This places the discrete pole at the analog pole's exact image
/// (`e^(−dt/RC)`), so the −3 dB corner lands on `1/(2π·RC)` to a fraction of a percent even
/// for a treble corner — unlike the cruder backward-Euler `dt/(RC+dt)`, which sags several
/// percent there. The `exp` is paid **once** here at construction; `process` is unchanged.
///
/// Coefficient and state are `f64` (the accumulator policy — state feeds back every sample).
/// `process` is the hot path: zero-alloc, panic-free, denormals flushed.
#[derive(Debug, Clone)]
pub struct OnePole {
    /// Smoothing coefficient `a = 1 − e^(−dt/RC)` ∈ [0, 1].
    a: f64,
    /// Filter state: the running output (the capacitor voltage).
    y: f64,
}

impl OnePole {
    /// Build from the resistance the cap sees (`r_thev`) and the capacitance, at `rate`.
    ///
    /// Limits behave gracefully: `RC → 0` (no cable / no cap) ⇒ `a → 1` (pass-through);
    /// `RC → ∞` ⇒ `a → 0` (frozen). `C = 0` gives `RC = 0`, `dt/RC = +∞`, `a = 1`.
    #[must_use]
    pub fn new(r_thev: Ohms, c: Farads, rate: AnalogRate) -> Self {
        let dt = rate.seconds_per_sample();
        let rc = f64::from(r_thev.get()) * f64::from(c.get());
        let a = 1.0 - (-dt / rc).exp();
        Self { a, y: 0.0 }
    }

    /// Clear the filter state. Off the hot path.
    pub fn reset(&mut self) {
        self.y = 0.0;
    }

    /// Process a block of voltage in place. Zero-alloc, panic-free, denormals flushed.
    pub fn process(&mut self, buf: &mut VoltageBuffer) {
        self.process_slice(buf.as_mut_slice());
    }

    /// Process a raw sample slice in place — the same hot-path filter as [`process`](Self::process),
    /// for callers that already hold a slice (e.g. an edge transform). Zero-alloc, panic-free,
    /// denormals flushed.
    pub(crate) fn process_slice(&mut self, samples: &mut [f32]) {
        for s in samples {
            *s = self.step(f64::from(*s)) as f32;
        }
    }

    /// Advance the filter by one sample: feed input `x`, return the new low-passed output
    /// `y[n] = y[n-1] + a·(x[n] − y[n-1])`. The single place the recurrence lives — both the
    /// in-place low-pass ([`process_slice`](Self::process_slice)) and the high-pass
    /// [`DcBlocker`](crate::DcBlocker) (which outputs `x − y`) run through it, so the two
    /// filters share one pole without inheritance. Hot path: `#[inline]`, denormal-flushed.
    #[inline]
    pub(crate) fn step(&mut self, x: f64) -> f64 {
        self.y += self.a * (x - self.y);
        self.y = flush_denormal(self.y);
        self.y
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::electrical::divider_gain;
    use crate::signal::Volts;
    use crate::test_util::measure_gain;
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    #[test]
    fn no_capacitance_is_pass_through() {
        // C = 0 ⇒ a = 1 ⇒ the filter is the identity, flat at every frequency.
        let mut f = OnePole::new(Ohms::new(10_000.0), Farads::ZERO, rate());
        let g = measure_gain(10_000.0, rate(), |buf| f.process(buf));
        assert_relative_eq!(g, 1.0, epsilon = 1e-3);
    }

    #[test]
    fn corner_is_minus_3_db_at_the_computed_frequency() {
        // R_thev = 10 kΩ, C = 1 nF  →  f_c = 1 / (2π·10000·1e-9) = 15_915.5 Hz.
        // A one-pole is −3 dB (gain 0.707) at f_c, ≈ unity a decade below, rolling off above.
        let f_c = 15_915.5;
        let one_pole = || OnePole::new(Ohms::new(10_000.0), Farads::new(1e-9), rate());

        let mut at_corner = one_pole();
        let g_c = measure_gain(f_c, rate(), |buf| at_corner.process(buf));
        assert_relative_eq!(g_c, 0.707_106_77, epsilon = 1.5e-2);

        let mut decade_below = one_pole();
        let g_lo = measure_gain(f_c / 10.0, rate(), |buf| decade_below.process(buf));
        assert!(
            g_lo > 0.98,
            "passband should be ~unity well below f_c, got {g_lo}"
        );

        let mut above = one_pole();
        let g_hi = measure_gain(f_c * 4.0, rate(), |buf| above.process(buf));
        assert!(
            g_hi < 0.30,
            "should be well into rolloff above f_c, got {g_hi}"
        );
    }

    #[test]
    fn a_longer_cable_loses_more_treble() {
        // Same source/load; only the cable capacitance differs. More C ⇒ lower corner ⇒
        // more loss at a fixed treble frequency. (Longer cable = more pF = darker.)
        let z_out = Ohms::new(10_000.0);
        let load = InputZ::new(Ohms::new(1_000_000.0));
        let test_hz = 15_000.0;

        let short = Cable::new(Ohms::new(100.0), Farads::new(1e-9));
        let long = Cable::new(Ohms::new(100.0), Farads::new(4e-9));

        let mut sf = short.lowpass(z_out, load, rate());
        let mut lf = long.lowpass(z_out, load, rate());
        let g_short = measure_gain(test_hz, rate(), |buf| sf.process(buf));
        let g_long = measure_gain(test_hz, rate(), |buf| lf.process(buf));
        assert!(
            g_long < g_short,
            "longer cable should lose more treble: long {g_long} vs short {g_short}"
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut f = OnePole::new(Ohms::new(10_000.0), Farads::new(1e-9), rate());
        let mut warm = VoltageBuffer::zeros(64, rate());
        warm.fill(Volts::new(1.0)); // drive the state up toward 1 V
        f.process(&mut warm);

        f.reset();
        let mut silence = VoltageBuffer::zeros(8, rate());
        f.process(&mut silence); // with state cleared, silent in ⇒ silent out
        assert!(silence.as_slice().iter().all(|&v| v == 0.0));
    }

    #[test]
    fn capstone_divider_loss_and_treble_rolloff_compose() {
        // High-Z source → cable → a 10 kΩ load: the resistive divider AND the cable LPF
        // act together. The passband gain should equal the divider loss; the corner should
        // be that loss × 0.707 — proving the constant resistive divider and the unity-DC
        // one-pole compose into the full shunt-C divider response.
        let z_out = Ohms::new(10_000.0);
        let cable = Cable::new(Ohms::new(100.0), Farads::new(2e-9));
        let load = InputZ::new(Ohms::new(10_000.0));

        // Resistive loss: 10000 / (10000 + 100 + 10000) = 0.49751.
        let g_div = divider_gain(z_out, cable.r(), load);
        assert_relative_eq!(g_div, 0.497_512, epsilon = 1e-5);

        // R_thev = (10000+100) ∥ 10000 = 5024.88 Ω; f_c = 1/(2π·5024.88·2e-9) = 15_838 Hz.
        let f_c = 15_838.0;

        // The full edge = scale by the divider gain, then run the cable filter.
        let mut lp_lo = cable.lowpass(z_out, load, rate());
        let g_passband = measure_gain(1_000.0, rate(), |buf| {
            for s in buf.as_mut_slice() {
                *s *= g_div;
            }
            lp_lo.process(buf);
        });
        // Well below f_c the filter passes → the chain is just the resistive loss.
        assert_relative_eq!(g_passband, g_div, epsilon = 1e-2);

        let mut lp_c = cable.lowpass(z_out, load, rate());
        let g_corner = measure_gain(f_c, rate(), |buf| {
            for s in buf.as_mut_slice() {
                *s *= g_div;
            }
            lp_c.process(buf);
        });
        // At the corner the chain is the resistive loss × the one-pole's 0.707.
        assert_relative_eq!(g_corner, g_div * 0.707_106_77, epsilon = 1.5e-2);
        assert!(
            g_corner < g_passband,
            "the corner must sit below the passband"
        );
    }
}
