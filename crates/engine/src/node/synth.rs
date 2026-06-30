//! A monophonic synth voice: the first node driven by *both* input lanes.

use super::Node;
use crate::electrical::{Ohms, OutputZ};
use crate::param::{ParamDecl, ParamId, Params};
use crate::port::{EventFace, InputPort, OutputPort};
use crate::signal::{AnalogRate, EventMessage, Lane, Volts};

/// MIDI note 69 (A4) is the 440 Hz reference; each semitone is a factor of `2^(1/12)`.
fn note_to_freq(note: u8) -> f64 {
    440.0 * 2.0_f64.powf((f64::from(note) - 69.0) / 12.0)
}

/// Which segment of the ADSR contour the envelope is on.
#[derive(Clone, Copy, PartialEq)]
enum Stage {
    /// Silent, awaiting a gate.
    Idle,
    /// Rising to full level.
    Attack,
    /// Falling from full to the sustain level.
    Decay,
    /// Holding the sustain level while the note is gated on.
    Sustain,
    /// Falling to silence after the gate releases.
    Release,
}

/// A simple **monophonic** synth voice: a sawtooth oscillator shaped by a linear ADSR envelope,
/// the first node that consumes *both* engine input lanes.
///
/// - **Events** (the [`Events`](crate::Domain::Events) input) carry pitch and gate: a note-on sets
///   the frequency and (re)triggers the envelope; a note-off for the sounding note releases it. It
///   is **last-note priority** — a new note-on always takes over, and a note-off for a note already
///   superseded is ignored (so trills don't cut the held note). A bare `Gate` retriggers/releases
///   at the current pitch.
/// - **Control params** (smoothed) are the knobs: output [`LEVEL`](Self::LEVEL) (volts, de-zippered
///   like a volume fader) and the envelope's [`ATTACK_MS`](Self::ATTACK_MS) /
///   [`DECAY_MS`](Self::DECAY_MS) / [`SUSTAIN`](Self::SUSTAIN) / [`RELEASE_MS`](Self::RELEASE_MS),
///   read at each gate transition, plus a [`POWERED`](Self::POWERED) switch that gates the output to
///   silence when off (de-clicked, never a recompile).
///
/// The oscillator runs in the oversampled analog domain, so a naive sawtooth's harmonics extend up
/// toward the analog Nyquist; the modeled **AD converter's** anti-alias filter removes
/// the ones that would fold — no band-limiting tricks are needed here, which is the converter
/// payoff, not a special case. Instruments are deliberately simple (PROJECT_PLAN §6): recognizable,
/// not realistic. No analog input; one analog output with a real source impedance.
pub struct SynthVoice {
    rate_hz: f64,
    /// Oscillator phase in `[0, 1)`.
    phase: f64,
    /// Current oscillator frequency (Hz); 0 until the first note.
    freq_hz: f64,
    /// The sounding note, for last-note priority; `None` when released.
    note: Option<u8>,
    stage: Stage,
    /// Current envelope level in `[0, 1]`.
    env: f32,
    /// Per-sample envelope increments, recomputed from the params at each gate transition.
    attack_inc: f32,
    decay_inc: f32,
    sustain: f32,
    release_inc: f32,
    /// Fallback output level (volts) when run without a schedule (the [`LEVEL`](Self::LEVEL) default).
    default_level: f32,
    params: [ParamDecl; 6],
    inputs: [InputPort; 1],
    outputs: [OutputPort; 1],
}

impl SynthVoice {
    /// Output level (volts) — a smoothed volume knob.
    pub const LEVEL: ParamId = ParamId(0);
    /// Envelope attack time (ms): time to rise from 0 to full.
    pub const ATTACK_MS: ParamId = ParamId(1);
    /// Envelope decay time (ms): time to fall from full to the sustain level.
    pub const DECAY_MS: ParamId = ParamId(2);
    /// Envelope sustain level (0–1): the held fraction of full while gated on.
    pub const SUSTAIN: ParamId = ParamId(3);
    /// Envelope release time (ms): time to fall from the current level to silence after release.
    pub const RELEASE_MS: ParamId = ParamId(4);
    /// Power switch (`0` = off, `1` = on). A powered-off voice emits silence — its output is gated to
    /// zero. The smoothed value de-clicks the on/off transition, so a toggle is glitch-free without
    /// being a structural graph edit. Defaults on (`1.0`).
    pub const POWERED: ParamId = ParamId(5);

    /// How many events the voice's input lane buffers per block — generous for a single voice.
    const EVENT_CAPACITY: usize = 64;

    /// A voice with default ADSR (5 ms / 10 ms / 0.7 / 10 ms) at output level `level`, driving from
    /// `z_out`. The level and ADSR are control params — drive them with `(node, SynthVoice::LEVEL)`
    /// etc.; uncontrolled, they hold these defaults.
    #[must_use]
    pub fn new(level: Volts, z_out: Ohms) -> Self {
        let default_level = level.get();
        Self {
            rate_hz: 0.0,
            phase: 0.0,
            freq_hz: 0.0,
            note: None,
            stage: Stage::Idle,
            env: 0.0,
            attack_inc: 0.0,
            decay_inc: 0.0,
            sustain: 0.7,
            release_inc: 0.0,
            default_level,
            params: [
                ParamDecl {
                    id: Self::LEVEL,
                    default: default_level,
                    // A line-level instrument output: 0 V (silent) up to 1.5 V (hot), default 1 V; the
                    // useful musical range is ~0.1–1.5 V. (Was 100 V — absurd for a line output and it
                    // left the whole usable range in the bottom 1.5% of the fader.)
                    min: 0.0,
                    max: 1.5,
                    smooth_ms: 5.0,
                },
                // The envelope-time params snap (smooth_ms 0): smoothing a *time* is meaningless,
                // and a change should take its next gate at face value.
                ParamDecl {
                    id: Self::ATTACK_MS,
                    default: 5.0,
                    min: 0.0,
                    max: 10_000.0,
                    smooth_ms: 0.0,
                },
                ParamDecl {
                    id: Self::DECAY_MS,
                    default: 10.0,
                    min: 0.0,
                    max: 10_000.0,
                    smooth_ms: 0.0,
                },
                ParamDecl {
                    id: Self::SUSTAIN,
                    default: 0.7,
                    min: 0.0,
                    max: 1.0,
                    smooth_ms: 0.0,
                },
                ParamDecl {
                    id: Self::RELEASE_MS,
                    default: 10.0,
                    min: 0.0,
                    max: 10_000.0,
                    smooth_ms: 0.0,
                },
                // Power gates the output; the 5 ms smoothing de-clicks the on/off step.
                ParamDecl {
                    id: Self::POWERED,
                    default: 1.0,
                    min: 0.0,
                    max: 1.0,
                    smooth_ms: 5.0,
                },
            ],
            inputs: [EventFace::new(Self::EVENT_CAPACITY).into()],
            outputs: [OutputZ::new(z_out).into()],
        }
    }

    /// Samples to cover `span` of envelope level over `ms` at the current rate (≥ 1 sample, so a
    /// 0 ms time is an instant jump rather than a divide-by-zero).
    fn inc(&self, span: f32, ms: f32) -> f32 {
        let samples = (f64::from(ms) * 1e-3 * self.rate_hz).max(1.0);
        span / samples as f32
    }

    /// Begin (or retrigger) the note: read the attack/decay/sustain params at this sample and enter
    /// Attack from the current level (retrigger is click-free — it doesn't snap to zero).
    fn gate_on(&mut self, params: &Params, at: usize) {
        let attack_ms = params.value_at_or(Self::ATTACK_MS, at, 5.0);
        let decay_ms = params.value_at_or(Self::DECAY_MS, at, 10.0);
        self.sustain = params.value_at_or(Self::SUSTAIN, at, 0.7);
        self.attack_inc = self.inc(1.0, attack_ms);
        self.decay_inc = self.inc(1.0 - self.sustain, decay_ms);
        self.stage = Stage::Attack;
    }

    /// Release the note: fall linearly from the current level to 0 over the release time.
    fn gate_off(&mut self, params: &Params, at: usize) {
        let release_ms = params.value_at_or(Self::RELEASE_MS, at, 10.0);
        self.release_inc = self.inc(self.env, release_ms);
        self.stage = Stage::Release;
    }

    /// Apply one event at sample offset `at`.
    fn apply(&mut self, message: EventMessage, params: &Params, at: usize) {
        match message {
            EventMessage::NoteOn { note, .. } => {
                self.note = Some(note);
                self.freq_hz = note_to_freq(note);
                self.gate_on(params, at);
            }
            EventMessage::NoteOff { note } => {
                // Last-note priority: only the *currently sounding* note's release counts.
                if self.note == Some(note) {
                    self.note = None;
                    self.gate_off(params, at);
                }
            }
            EventMessage::Gate(true) => self.gate_on(params, at),
            EventMessage::Gate(false) => {
                self.note = None;
                self.gate_off(params, at);
            }
        }
    }

    /// The envelope level for this sample, then advance one sample along the ADSR contour.
    fn env_step(&mut self) -> f32 {
        let level = self.env;
        match self.stage {
            Stage::Idle => {}
            Stage::Attack => {
                self.env += self.attack_inc;
                if self.env >= 1.0 {
                    self.env = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                self.env -= self.decay_inc;
                if self.env <= self.sustain {
                    self.env = self.sustain;
                    self.stage = Stage::Sustain;
                }
            }
            Stage::Sustain => self.env = self.sustain,
            Stage::Release => {
                self.env -= self.release_inc;
                if self.env <= 0.0 {
                    self.env = 0.0;
                    self.stage = Stage::Idle;
                }
            }
        }
        level
    }
}

impl Node for SynthVoice {
    fn inputs(&self) -> &[InputPort] {
        &self.inputs
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    fn params(&self) -> &[ParamDecl] {
        &self.params
    }

    fn prepare(&mut self, rate: AnalogRate) {
        self.rate_hz = rate.as_hz();
    }

    fn process(&mut self, params: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
        let events = inputs[0].events().as_slice();
        let out = outputs[0].voltage_mut().as_mut_slice();
        let mut ei = 0;
        for (i, o) in out.iter_mut().enumerate() {
            // Apply any events scheduled at this sample (in order) before generating it — this is
            // where the sample-accurate timing lands.
            while ei < events.len() && events[ei].offset as usize <= i {
                self.apply(events[ei].message, params, i);
                ei += 1;
            }
            let level = params.value_at_or(Self::LEVEL, i, self.default_level);
            let powered = params.value_at_or(Self::POWERED, i, 1.0);
            let env = self.env_step();
            let saw = (2.0 * self.phase - 1.0) as f32; // naive ramp in [-1, 1)
            *o = saw * env * level * powered;
            self.phase += self.freq_hz / self.rate_hz;
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
    use crate::param::ParamQueue;
    use crate::schedule::{EventQueue, compile};
    use crate::signal::{EventMessage, VoltageBuffer};
    use crate::test_util::{rms, tone_amplitude};

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    /// note 69 = A4 = 440 Hz exactly (the tuning reference).
    #[test]
    fn note_69_is_440_hz() {
        assert!((note_to_freq(69) - 440.0).abs() < 1e-9);
        assert!((note_to_freq(81) - 880.0).abs() < 1e-9); // an octave up
    }

    /// A voice → tap chain (near-ideal output into a bridging tap), with a handle to its open event
    /// input and its level param. Returns the compiled schedule plus both handles.
    fn voice_chain(block: usize) -> (crate::Schedule, crate::EventInputId, crate::ParamHandle) {
        let mut g = Graph::new();
        let voice = g.add(SynthVoice::new(Volts::new(1.0), Ohms::new(1.0)));
        g.set_output(voice, 0);
        let sched = compile(g, block, rate(), 0).expect("valid voice chain");
        let ev = sched.event_input(voice, 0).expect("open event input");
        let lvl = sched.param(voice, SynthVoice::LEVEL).expect("level param");
        (sched, ev, lvl)
    }

    #[test]
    fn silent_until_triggered() {
        // No events ⇒ the envelope stays idle ⇒ pure silence, however long it runs.
        let (mut sched, _ev, _lvl) = voice_chain(256);
        let mut out = VoltageBuffer::zeros(256, rate());
        sched.process(&mut out);
        assert!(out.as_slice().iter().all(|&v| v == 0.0));
    }

    #[test]
    fn note_on_triggers_sample_accurately() {
        // A note-on at offset 100 in a 256-sample block: every sample before 100 is exactly silent,
        // and signal appears after — the gate lands on its sample, not the block boundary.
        let (mut sched, ev, _lvl) = voice_chain(256);
        let mut q = EventQueue::with_capacity(8);
        q.push(
            100,
            ev,
            EventMessage::NoteOn {
                note: 69,
                velocity: 100,
            },
        );
        let mut out = VoltageBuffer::zeros(256, rate());
        sched.process_with_events(&mut out, &mut q);

        let s = out.as_slice();
        assert!(
            s[..100].iter().all(|&v| v == 0.0),
            "must be silent before the trigger sample"
        );
        assert!(
            s[100..].iter().any(|&v| v != 0.0),
            "the note must sound after the trigger"
        );
    }

    #[test]
    fn sustained_note_oscillates_at_the_played_pitch() {
        // Hold note 69 (440 Hz) long enough to reach sustain, then read the steady tail: the 440 Hz
        // fundamental dominates a detuned (550 Hz) bin by a wide margin — the voice plays in tune.
        let block = 16_384;
        let (mut sched, ev, _lvl) = voice_chain(block);
        let mut q = EventQueue::with_capacity(8);
        q.push(
            0,
            ev,
            EventMessage::NoteOn {
                note: 69,
                velocity: 100,
            },
        );
        let mut out = VoltageBuffer::zeros(block, rate());
        sched.process_with_events(&mut out, &mut q);

        // Skip the attack/decay transient (≪ 4096 samples at 5 ms + 10 ms / 384 kHz).
        let tail = &out.as_slice()[block / 4..];
        let at_pitch = tone_amplitude(tail, 440.0, rate());
        let off_pitch = tone_amplitude(tail, 550.0, rate());
        assert!(
            at_pitch > off_pitch * 5.0,
            "the 440 Hz fundamental ({at_pitch}) should dominate a detuned bin ({off_pitch})"
        );
        assert!(at_pitch > 0.1, "the note should be clearly audible");
    }

    #[test]
    fn note_off_releases_to_silence() {
        // Trigger, let it sound, then release with a short tail: the output decays back to silence.
        let block = 8_192;
        let (mut sched, ev, _lvl) = voice_chain(block);
        let mut q = EventQueue::with_capacity(8);
        q.push(
            0,
            ev,
            EventMessage::NoteOn {
                note: 69,
                velocity: 100,
            },
        );
        // Release partway through; with a 10 ms tail it's silent well before the block ends.
        q.push(block as u64 / 4, ev, EventMessage::NoteOff { note: 69 });
        let mut out = VoltageBuffer::zeros(block, rate());
        sched.process_with_events(&mut out, &mut q);

        let last = &out.as_slice()[block - 1000..];
        assert!(
            rms(last) < 1e-4,
            "the voice should have released to silence, rms {}",
            rms(last)
        );
    }

    #[test]
    fn level_param_scales_the_output() {
        // Two sustained renders at different LEVELs: the louder one has the larger fundamental,
        // roughly in proportion — the smoothed volume knob works end to end.
        fn fundamental_at_level(level: f32) -> f32 {
            let block = 16_384;
            let (mut sched, ev, lvl) = voice_chain(block);
            let mut q = EventQueue::with_capacity(8);
            q.push(
                0,
                ev,
                EventMessage::NoteOn {
                    note: 69,
                    velocity: 100,
                },
            );
            let mut pq = ParamQueue::with_capacity(1);
            pq.set(lvl, level);
            let mut out = VoltageBuffer::zeros(block, rate());
            sched.process_io(&mut out, &mut pq, &mut q);
            tone_amplitude(&out.as_slice()[block / 4..], 440.0, rate())
        }
        let quiet = fundamental_at_level(0.3);
        let loud = fundamental_at_level(1.2);
        assert!(loud > quiet * 3.0, "4× the level ⇒ a much larger tone");
    }

    #[test]
    fn powered_off_silences_the_voice() {
        // Hold a note, but with POWERED driven to 0: the output gate ⇒ silence in the settled tail,
        // even though the envelope is sounding. The de-click glide (5 ms) is well past by block/4.
        let block = 16_384;
        let mut g = Graph::new();
        let voice = g.add(SynthVoice::new(Volts::new(1.0), Ohms::new(1.0)));
        g.set_output(voice, 0);
        let mut sched = compile(g, block, rate(), 0).expect("valid voice chain");
        let ev = sched.event_input(voice, 0).expect("open event input");
        let pwr = sched
            .param(voice, SynthVoice::POWERED)
            .expect("powered param");

        let mut q = EventQueue::with_capacity(8);
        q.push(
            0,
            ev,
            EventMessage::NoteOn {
                note: 69,
                velocity: 100,
            },
        );
        let mut pq = ParamQueue::with_capacity(1);
        pq.set(pwr, 0.0); // power off
        let mut out = VoltageBuffer::zeros(block, rate());
        sched.process_io(&mut out, &mut pq, &mut q);

        let tail = &out.as_slice()[block / 4..];
        assert!(
            rms(tail) < 1e-4,
            "a powered-off voice must be silent, rms {}",
            rms(tail)
        );
    }

    #[test]
    fn a_stale_note_off_does_not_cut_the_current_note() {
        // Last-note priority: play 60, then 67, then release 60. The 67 must keep sounding because
        // the note-off was for a note the voice already moved off of.
        let block = 12_288;
        let (mut sched, ev, _lvl) = voice_chain(block);
        let mut q = EventQueue::with_capacity(8);
        q.push(
            0,
            ev,
            EventMessage::NoteOn {
                note: 60,
                velocity: 100,
            },
        );
        q.push(
            10,
            ev,
            EventMessage::NoteOn {
                note: 67,
                velocity: 100,
            },
        );
        q.push(20, ev, EventMessage::NoteOff { note: 60 }); // stale: 60 was superseded by 67
        let mut out = VoltageBuffer::zeros(block, rate());
        sched.process_with_events(&mut out, &mut q);

        // Still sounding at 67's pitch (≈392 Hz), not released to silence.
        let tail = &out.as_slice()[block / 4..];
        assert!(rms(tail) > 0.05, "the current note must keep sounding");
        let g4 = tone_amplitude(tail, note_to_freq(67), rate());
        let c4 = tone_amplitude(tail, note_to_freq(60), rate());
        assert!(
            g4 > c4,
            "it should be sounding note 67, not the released 60"
        );
    }
}
