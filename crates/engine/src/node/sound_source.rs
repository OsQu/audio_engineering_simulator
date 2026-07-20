//! A generic acoustic sound source: the first node that *emits* sound pressure.

use super::Node;
use crate::param::{ParamDecl, ParamId, Params};
use crate::port::{AcousticFace, InputPort, OutputPort};
use crate::signal::{AnalogRate, Lane, Pascals};

/// A generic **acoustic sound source**: a sine at a chosen frequency, radiating at a chosen
/// loudness into the air. The first node whose output is a [`Pressure`](crate::Lane::Pressure)
/// lane — a source you *mic*, not a line-out.
///
/// Deliberately simple (PROJECT_PLAN §2): recognizable, not realistic. The sine is a stand-in for
/// whatever is making the sound (a voice, a drum, an amp); what matters for the simulation is that
/// it produces a real sound pressure at a real loudness, so the whole front-end story — mic choice,
/// phantom, gain staging, the pad — follows. Kept generic on purpose: the catalog dresses one
/// instance up as "vox", but a future drum/amp/guitar reuses the same node.
///
/// - **Loudness** is the [`LEVEL`](Self::LEVEL) control param: the sine's **peak pressure amplitude
///   in pascals** (linear, de-zippered like a volume fader). Loudness is quoted in **dB SPL** at the
///   UI (a whisper ~30 dB, a cranked amp ~120 dB); that log knob maps to this linear param exactly
///   as a gain fader's dB maps to [`GainStage`](super::GainStage)'s linear gain — the engine stores
///   the physical quantity, the knob is presentation. The calibration anchor is **1 Pa = 94 dB SPL**
///   (see [`spl_to_pascals`](crate::spl_to_pascals)).
/// - **Pitch** is the [`FREQ`](Self::FREQ) control param (Hz), smoothed so a change glides.
///
/// An `f64` phase accumulator stepped at the analog rate makes it reproducible with no ambient
/// entropy (structural determinism, like [`SynthVoice`](super::SynthVoice) and the mics). No inputs;
/// one acoustic output (mono — air is modelled ideally). Unprepared (no analog rate yet — `compile`
/// always prepares before `process`, so a compiled schedule never hits this), the phase step is 0
/// and the source is silent.
pub struct SoundSource {
    /// Analog rate (Hz), baked at [`prepare`](Node::prepare); `0.0` until prepared (source silent).
    rate_hz: f64,
    /// Oscillator phase in `[0, 1)`. Free-runs across blocks.
    phase: f64,
    /// The [`LEVEL`](Self::LEVEL) default (peak pressure amplitude, pascals) — the constructed level
    /// and the fallback when run outside a schedule.
    default_level: f32,
    /// The declared control params: [`LEVEL`](Self::LEVEL), [`FREQ`](Self::FREQ).
    params: [ParamDecl; 2],
    outputs: [OutputPort; 1],
}

impl SoundSource {
    /// Loudness: the sine's peak pressure amplitude (pascals) — a smoothed level knob. Uncontrolled,
    /// it holds the constructed level.
    pub const LEVEL: ParamId = ParamId(0);
    /// Pitch: the sine's frequency (Hz) — smoothed. Defaults to
    /// [`FREQ_DEFAULT_HZ`](Self::FREQ_DEFAULT_HZ).
    pub const FREQ: ParamId = ParamId(1);

    /// The default pitch: 220 Hz — a plausible voice/instrument fundamental (A3).
    pub const FREQ_DEFAULT_HZ: f32 = 220.0;
    /// Loudness ceiling (pascals): 100 Pa ≈ 134 dB SPL, above the loudest source we model (a
    /// cranked amp ~120 dB SPL = 20 Pa), so the whole micing range fits under it.
    const LEVEL_MAX_PASCALS: f32 = 100.0;
    /// Pitch knob bounds (Hz): the audible band.
    const FREQ_MIN_HZ: f32 = 20.0;
    const FREQ_MAX_HZ: f32 = 20_000.0;
    /// De-zipper glide for the loudness / pitch knobs — short enough to feel instant.
    const SMOOTH_MS: f32 = 5.0;

    /// A source radiating a sine of peak pressure `level` at [`FREQ_DEFAULT_HZ`](Self::FREQ_DEFAULT_HZ).
    ///
    /// `level` is a linear peak pressure — build it from a dB SPL loudness with
    /// [`spl_to_pascals`](crate::spl_to_pascals) (e.g. `spl_to_pascals(94.0)` ≈ 1 Pa).
    #[must_use]
    pub fn new(level: Pascals) -> Self {
        let default_level = level.get();
        Self {
            rate_hz: 0.0,
            phase: 0.0,
            default_level,
            params: [
                ParamDecl {
                    id: Self::LEVEL,
                    default: default_level,
                    min: 0.0,
                    max: Self::LEVEL_MAX_PASCALS,
                    smooth_ms: Self::SMOOTH_MS,
                },
                ParamDecl {
                    id: Self::FREQ,
                    default: Self::FREQ_DEFAULT_HZ,
                    min: Self::FREQ_MIN_HZ,
                    max: Self::FREQ_MAX_HZ,
                    smooth_ms: Self::SMOOTH_MS,
                },
            ],
            outputs: [AcousticFace::new().into()],
        }
    }
}

impl Node for SoundSource {
    fn inputs(&self) -> &[InputPort] {
        &[]
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    fn params(&self) -> &[ParamDecl] {
        &self.params
    }

    fn prepare(&mut self, rate: AnalogRate) {
        // Bake the analog rate for the phase step; the phase itself is left untouched so it keeps
        // free-running across a recompile (matching `SynthVoice`/the mics).
        self.rate_hz = rate.as_hz();
    }

    fn process(&mut self, params: &Params, _inputs: &[Lane], outputs: &mut [Lane]) {
        // `inv_rate` is 0 when unprepared, freezing the phase (source silent) — the only "off"
        // state this node has. The peak-pressure amplitude is a linear param, multiplied straight
        // in (the dB-SPL knob mapping lives at the UI), so the hot loop is one sine per sample.
        let inv_rate = if self.rate_hz > 0.0 {
            1.0 / self.rate_hz
        } else {
            0.0
        };
        let out = outputs[0].pressure_mut().as_mut_slice();
        for (i, o) in out.iter_mut().enumerate() {
            let level = params.value_at_or(Self::LEVEL, i, self.default_level);
            let freq = params.value_at_or(Self::FREQ, i, Self::FREQ_DEFAULT_HZ);
            *o = level * (std::f64::consts::TAU * self.phase).sin() as f32;
            // Free-run the phase, wrapping in [0, 1) like `SynthVoice`/the mics.
            self.phase += f64::from(freq) * inv_rate;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Graph;
    use crate::level::{pascals_to_spl, spl_to_pascals};
    use crate::schedule::compile;
    use crate::signal::PressureBuffer;
    use approx::assert_relative_eq;

    const RATE_HZ: f64 = 384_000.0;

    fn rate() -> AnalogRate {
        AnalogRate::new(RATE_HZ)
    }

    /// A prepared source at loudness `level`, processed for `n` samples outside a schedule; returns
    /// the emitted pressure block.
    fn run_source(level: Pascals, n: usize) -> PressureBuffer {
        let mut s = SoundSource::new(level);
        s.prepare(rate());
        let mut out = [Lane::Pressure(PressureBuffer::zeros(n, rate()))];
        s.process(&Params::EMPTY, &[], &mut out);
        match out.into_iter().next().unwrap() {
            Lane::Pressure(b) => b,
            _ => unreachable!(),
        }
    }

    /// The peak |pressure| over a block.
    fn peak(buf: &PressureBuffer) -> f32 {
        buf.as_slice().iter().fold(0.0_f32, |m, &p| m.max(p.abs()))
    }

    #[test]
    fn declares_a_mono_acoustic_output_and_no_input() {
        let s = SoundSource::new(spl_to_pascals(94.0));
        assert!(s.inputs().is_empty());
        assert_eq!(s.outputs()[0].domain(), crate::Domain::Acoustic);
        assert_eq!(s.outputs()[0].lane_count(), 1); // mono
        // Two smoothed knobs: LEVEL (Pa) and FREQ (default 220 Hz).
        let ps = s.params();
        assert_eq!(ps.len(), 2);
        assert_relative_eq!(ps[1].default, 220.0);
    }

    #[test]
    fn emits_a_sine_whose_peak_is_the_spl_loudness() {
        // 94 dB SPL ⇒ 1 Pa (the calibration anchor): the sine's peak pressure ≈ 1 Pa. Read the
        // peak back as SPL and confirm it lands at 94 dB. A long-enough block spans many periods of
        // 220 Hz at 384 kHz (period ≈ 1745 samples) so the peak nears the true amplitude.
        let buf = run_source(spl_to_pascals(94.0), 8192);
        assert_relative_eq!(peak(&buf), 1.0024, epsilon = 1e-2); // spl_to_pascals(94) = 1.0024 Pa
        assert_relative_eq!(
            pascals_to_spl(Pascals::new(peak(&buf))),
            94.0,
            epsilon = 0.1
        );
    }

    #[test]
    fn a_cranked_amp_is_20_pascals() {
        // 120 dB SPL ⇒ 20 Pa exactly — the loud end of the micing range.
        let buf = run_source(spl_to_pascals(120.0), 8192);
        assert_relative_eq!(peak(&buf), 20.0, epsilon = 0.1);
    }

    #[test]
    fn silent_until_prepared() {
        // No `prepare` ⇒ no analog rate ⇒ the phase step is 0: the sine of a frozen phase-0 is 0,
        // so every sample is exactly silent. (compile always prepares, so a real schedule never
        // runs this path.)
        let mut s = SoundSource::new(spl_to_pascals(94.0));
        let mut out = [Lane::Pressure(PressureBuffer::zeros(4, rate()))];
        s.process(&Params::EMPTY, &[], &mut out);
        assert!(out[0].pressure().as_slice().iter().all(|&p| p == 0.0));
    }

    #[test]
    fn phase_free_runs_and_compiles_as_a_tap() {
        // It compiles as an acoustic output tap (pool inspection only) and its phase continues
        // across blocks: two 512-sample blocks tile a single 1024-sample reference run bit-for-bit.
        let reference = run_source(spl_to_pascals(100.0), 1024);
        let mut s = SoundSource::new(spl_to_pascals(100.0));
        s.prepare(rate());
        let mut b1 = [Lane::Pressure(PressureBuffer::zeros(512, rate()))];
        s.process(&Params::EMPTY, &[], &mut b1);
        let mut b2 = [Lane::Pressure(PressureBuffer::zeros(512, rate()))];
        s.process(&Params::EMPTY, &[], &mut b2);
        for k in 0..512 {
            assert_eq!(b1[0].pressure().get(k).get(), reference.get(k).get());
            assert_eq!(b2[0].pressure().get(k).get(), reference.get(k + 512).get());
        }

        // And it is a legal graph node (compiles with an acoustic tap set).
        let mut g = Graph::new();
        let src = g.add(SoundSource::new(spl_to_pascals(94.0)));
        g.set_output(src, 0);
        assert!(compile(g, 256, rate(), 0).is_ok());
    }
}
