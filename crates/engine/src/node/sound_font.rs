//! A SoundFont-based instrument voice: the first node driven by the event lane that emits on the
//! **digital** side, the black-box core of Epic 9's played instruments.

use std::error::Error;
use std::fmt;
use std::io::Cursor;
use std::sync::Arc;

use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};

use super::Node;
use crate::param::Params;
use crate::port::{AudioFormat, DigitalFace, EventFace, InputPort, OutputPort};
use crate::signal::{AnalogRate, BitDepth, EventMessage, Lane, SampleRate};

/// A construct-time failure building a [`SoundFontSource`]: the SoundFont bytes failed to parse (a
/// bad/rejected `.sf2` — `rustysynth` runs a structural sanity check at load), or the synthesizer
/// could not be built from them. Never returned on the hot path — all parsing/allocation is here.
///
/// The inner `rustysynth` error is deliberately **not** carried, so no `rustysynth` type leaks into
/// this crate's public API (the dependency stays isolated behind this module).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundFontError {
    /// The `.sf2` bytes could not be parsed / failed the structural sanity check.
    Parse,
    /// The synthesizer could not be constructed from the parsed font (e.g. degenerate settings).
    Build,
}

impl fmt::Display for SoundFontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SoundFontError::Parse => f.write_str("SoundFont bytes failed to parse"),
            SoundFontError::Build => {
                f.write_str("could not build a synthesizer from the SoundFont")
            }
        }
    }
}

impl Error for SoundFontError {}

/// A **SoundFont instrument voice**: MIDI note-on/off + velocity in (the event lane), a recognizable
/// polyphonic General-MIDI voice out (the digital [`Sample`](crate::Lane::Sample) lane @ 48 kHz).
///
/// Modeled honestly as a **digital sound module** (`PROJECT_PLAN` §5.6): SoundFont sample playback
/// at its own digital rate, which the downstream [`DaConverter`](super::DaConverter) lifts to analog
/// volts (48 kHz → 8× → the 384 kHz analog rate — a fixed integer ratio, no fractional resampler).
/// The DA is the domain bridge, exactly as everywhere else; this node stays entirely on the digital
/// side and never produces volts itself.
///
/// The DSP is delegated wholesale to [`rustysynth`](https://crates.io/crates/rustysynth), the
/// black-box synth core (`PROJECT_PLAN` §3 permits delegating internal DSP): it owns voice
/// allocation / stealing, velocity layers, and the SoundFont sample engine. This satisfies Epic 1's
/// deferred *polyphony / voice allocation* item by delegation. Instruments are deliberately
/// **recognizable, not realistic** (`PROJECT_PLAN` §6) — the realism budget stays on the
/// volts-and-converters layer downstream.
///
/// - **Events** (the [`Events`](crate::Domain::Events) input) carry the note lifecycle:
///   [`NoteOn`](EventMessage::NoteOn) starts a voice at its pitch + velocity, [`NoteOff`] releases
///   it. Timing is honored by **sub-slicing the render at each event's digital frame**, so a note
///   lands near its sample — though `rustysynth` applies note changes at its internal `block_size`
///   granularity (64 frames ≈ 1.3 ms @ 48 kHz), the honest control-rate of a sample module.
///   [`Gate`](EventMessage::Gate) events are ignored (a bare gate carries no pitch).
/// - **No control params** in this first cut: the GM preset is fixed at construction (selection is a
///   later config concern), loudness rides the DA reference / downstream gain, and velocity rides the
///   event. So [`params`](Node::params) is empty.
///
/// Deterministic: `rustysynth`'s render path has no RNG and touches no clock; reverb/chorus are left
/// **off** here (deterministic either way, and a clean release-to-silence tail matters more than
/// richness at this headless stage). Construction parses ~6 MB of font bytes and pre-allocates the
/// voice pool; [`process`](Node::process) only reads those tables — no allocation, no panic.
///
/// [`NoteOff`]: EventMessage::NoteOff
pub struct SoundFontSource {
    /// The black-box synth. Owns its `Arc<SoundFont>` and a voice pool pre-allocated at construction;
    /// its `render` grows nothing (proven zero-alloc by the Story 9.1 spike).
    synth: Synthesizer,
    /// Interpolation/decimation factor `M = analog rate / 48 kHz`, baked at [`prepare`](Node::prepare);
    /// an event at analog offset `o` maps to digital frame `o / M`. `1` until prepared.
    m: usize,
    inputs: [InputPort; 1],
    outputs: [OutputPort; 1],
}

impl SoundFontSource {
    /// The pinned internal digital rate (Hz): 48 kHz, so the ratio to the 384 kHz analog rate is
    /// exactly **8×** — reusing the shipped `DaConverter`/`Interpolator` path with no fractional
    /// resampler (44.1 kHz would force one we don't have and don't want).
    const RATE_HZ: i32 = 48_000;
    /// Nominal bit depth stamped on the digital output face. The samples are full-precision floats
    /// (this node has no AD quantizer in front of it); the depth only names the format the
    /// downstream `DaConverter` reads.
    const BITS: u32 = 24;
    /// Per-block event capacity — generous for one instrument voice (a dense chord + releases).
    const EVENT_CAPACITY: usize = 128;
    /// Stereo render scratch length (frames). Matches `rustysynth`'s default `block_size`, so a full
    /// internal block is drained per chunk. On the **stack** — no heap scratch (which
    /// [`prepare`](Node::prepare) couldn't size anyway, as it isn't told the block length).
    const CHUNK: usize = 64;

    /// Build a voice from raw `.sf2` `sf2_bytes`, playing GM program `preset` (0 = piano). Parses the
    /// font and constructs the synth **now** (fallible, allocating) so [`process`](Node::process)
    /// stays total; the bytes are read from a `Cursor` (never a `File`), the exact wasm path.
    ///
    /// # Errors
    /// [`SoundFontError::Parse`] if the bytes aren't a valid `.sf2` (or fail `rustysynth`'s structural
    /// sanity check), [`SoundFontError::Build`] if the synthesizer can't be built from them.
    pub fn new(sf2_bytes: &[u8], preset: u8) -> Result<Self, SoundFontError> {
        let mut cursor = Cursor::new(sf2_bytes);
        let sound_font = Arc::new(SoundFont::new(&mut cursor).map_err(|_| SoundFontError::Parse)?);

        let mut settings = SynthesizerSettings::new(Self::RATE_HZ);
        settings.enable_reverb_and_chorus = false;
        let mut synth =
            Synthesizer::new(&sound_font, &settings).map_err(|_| SoundFontError::Build)?;

        // Select the GM preset once, at construction (the event lane is note-only — program change is
        // not carried on it). A Program Change (0xC0) on channel 0; `note_on` resolves it through a
        // safe lookup with a GM fallback, so an out-of-set `preset` degrades to the default, never panics.
        synth.process_midi_message(0, 0xC0, i32::from(preset), 0);

        Ok(Self {
            synth,
            m: 1,
            inputs: [EventFace::new(Self::EVENT_CAPACITY).into()],
            outputs: [DigitalFace::new(AudioFormat::new(
                SampleRate::new(f64::from(Self::RATE_HZ)),
                BitDepth::new(Self::BITS),
                1,
            ))
            .into()],
        })
    }

    /// Start a note. Clamps key/velocity into the 7-bit MIDI range as a belt-and-suspenders guard
    /// (a malformed producer could send `u8` values above 127); `rustysynth` range-checks internally,
    /// so this never panics. Velocity 0 is a note-off, per MIDI running status.
    fn note_on(&mut self, key: u8, velocity: u8) {
        self.synth
            .note_on(0, i32::from(key.min(127)), i32::from(velocity.min(127)));
    }

    /// Release a note (clamped like [`note_on`](Self::note_on)).
    fn note_off(&mut self, key: u8) {
        self.synth.note_off(0, i32::from(key.min(127)));
    }

    /// Render `n` mono frames into `out[at..at + n]`: render `rustysynth`'s stereo output in
    /// `CHUNK`-sized stack pieces, fold L+R to mono (average, so a centered voice stays at unity),
    /// and clamp to full scale ±1.0 — this node *is* the module's internal full-scale digital bus,
    /// so a reverb/transient peak above ±1 clips here rather than leaking a >full-scale sample. Hot
    /// path: no allocation (stack scratch), no panic.
    fn render_into(&mut self, out: &mut [f32], at: usize, n: usize) {
        let mut left = [0.0_f32; Self::CHUNK];
        let mut right = [0.0_f32; Self::CHUNK];
        let mut done = 0;
        while done < n {
            let k = (n - done).min(Self::CHUNK);
            self.synth.render(&mut left[..k], &mut right[..k]);
            for j in 0..k {
                out[at + done + j] = (0.5 * (left[j] + right[j])).clamp(-1.0, 1.0);
            }
            done += k;
        }
    }
}

impl Node for SoundFontSource {
    fn inputs(&self) -> &[InputPort] {
        &self.inputs
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    fn prepare(&mut self, rate: AnalogRate) {
        // M = analog rate / digital rate. The integer-divide and block-multiple constraints were
        // already validated when `compile` sized this node's digital output lane (the AD's path).
        self.m = (rate.as_hz() / f64::from(Self::RATE_HZ)).round().max(1.0) as usize;
    }

    fn process(&mut self, _params: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
        let events = inputs[0].events().as_slice();
        let out = outputs[0].sample_mut().as_mut_slice();
        let m = self.m.max(1);

        // Dispatch each event at its digital frame: render up to it, apply it, continue. Events
        // arrive in nondecreasing offset order, so `cursor` only advances.
        let mut cursor = 0;
        for ev in events {
            let frame = (ev.offset as usize / m).min(out.len());
            if frame > cursor {
                self.render_into(out, cursor, frame - cursor);
                cursor = frame;
            }
            match ev.message {
                EventMessage::NoteOn { note, velocity } => self.note_on(note, velocity),
                EventMessage::NoteOff { note } => self.note_off(note),
                EventMessage::Gate(_) => {}
            }
        }
        // The tail after the last event (or the whole block if there were none).
        if cursor < out.len() {
            self.render_into(out, cursor, out.len() - cursor);
        }
    }
}
