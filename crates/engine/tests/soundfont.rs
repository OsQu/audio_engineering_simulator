//! Story 9.2 — the `SoundFontSource` node.
//!
//! - Task 9.2.1 drives the node **alone** (no schedule): parse a real GM font, play a note through
//!   `process`, and confirm a recognizable, finite, non-silent signal lands on the digital `Sample`
//!   lane.
//! - Task 9.2.2 wires the **headless chassis** — `SoundFontSource → DaConverter` in a compiled
//!   `Graph` — and confirms note events become recognizable, correctly-pitched analog **volts**.
//!
//! `test_util`'s `rms` / `tone_amplitude` are `#[cfg(test)]`-internal to the crate and unreachable
//! from this (external) integration test, so minimal local copies live at the bottom of the file.

use engine::{
    AnalogRate, BitDepth, ClockDomainId, DaConverter, EventBuffer, EventInputId, EventMessage,
    EventQueue, Graph, Lane, Node, Ohms, Params, SampleBuffer, SampleRate, Schedule,
    SoundFontError, SoundFontSource, TimedEvent, VoltageBuffer, Volts, compile,
};

/// The analog rate the engine runs at; 384 kHz / 48 kHz = 8× to the synth's digital rate.
fn rate() -> AnalogRate {
    AnalogRate::new(384_000.0)
}

/// GM drawbar organ — a **sustained** preset (unlike the decaying piano), so a held note gives a
/// steady tone the single-bin DFT can read cleanly for the pitch/release checks.
const GM_ORGAN: u8 = 16;

/// The shared GM SoundFont, read at test time (native only — the wasm byte seam is Story 9.3). Lives
/// at the repo root so the 9.4 web build can serve the same asset.
fn soundfont_bytes() -> Vec<u8> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/soundfonts/TimGM6mb.sf2"
    );
    std::fs::read(path).expect("the vendored TimGM6mb.sf2 asset")
}

/// Build a node output/input pair and run `process` once over a `frames`-long digital block, with
/// `events` applied at their offsets. Returns the mono sample block the node emitted.
fn render_block(node: &mut SoundFontSource, frames: usize, events: &[TimedEvent]) -> Vec<f32> {
    let mut ev = EventBuffer::with_capacity(events.len().max(1));
    for &e in events {
        ev.push(e);
    }
    let inputs = [Lane::Events(ev)];
    let mut outputs = [Lane::Sample(SampleBuffer::zeros(
        frames,
        SampleRate::new(48_000.0),
        BitDepth::new(24),
        ClockDomainId::SINGLE,
    ))];
    node.process(&Params::EMPTY, &inputs, &mut outputs);
    outputs[0].sample().as_slice().to_vec()
}

#[test]
fn plays_a_recognizable_note_onto_the_sample_lane() {
    // Middle C (MIDI 60) on the default GM preset (piano). 4096 frames @ 48 kHz ≈ 85 ms — well past
    // the attack, so the note has clearly sounded.
    let mut node = SoundFontSource::new(&soundfont_bytes(), 0).expect("valid GM soundfont");
    node.prepare(rate());

    let note_on = TimedEvent {
        offset: 0,
        message: EventMessage::NoteOn {
            note: 60,
            velocity: 100,
        },
    };
    let out = render_block(&mut node, 4096, &[note_on]);

    assert!(
        out.iter().all(|s| s.is_finite()),
        "every sample must be finite"
    );
    let peak = out.iter().fold(0.0_f32, |m, &s| m.max(s.abs()));
    assert!(
        peak > 1e-3,
        "the note should be clearly audible, got peak {peak}"
    );
    // Full-scale clamp holds — the node is the module's internal digital bus.
    assert!(
        peak <= 1.0,
        "the digital bus must stay within full scale, got peak {peak}"
    );
}

#[test]
fn silent_with_no_events() {
    // No note-on ⇒ no active voice ⇒ exact digital silence, however long we render.
    let mut node = SoundFontSource::new(&soundfont_bytes(), 0).expect("valid GM soundfont");
    node.prepare(rate());
    let out = render_block(&mut node, 1024, &[]);
    assert!(
        out.iter().all(|&s| s == 0.0),
        "an untriggered voice must be silent"
    );
}

#[test]
fn garbage_bytes_error_at_construction_without_panicking() {
    // A bad `.sf2` must surface a parse error, not panic — all fallibility lives at construction.
    match SoundFontSource::new(b"not a soundfont", 0) {
        Err(e) => assert_eq!(e, SoundFontError::Parse),
        Ok(_) => panic!("garbage bytes must not parse into a soundfont"),
    }
}

// ---- Task 9.2.2: the headless chassis (SoundFontSource → DaConverter → volts) ------------------

/// A compiled `SoundFontSource → DaConverter` chassis playing `preset`, over a `block`-sample analog
/// block, plus a handle to the source's open event input. The 1.0 V DA reference maps full scale to
/// ±1 V (provisional — the real instrument-out calibration is Story 9.4). `block` must be a multiple
/// of 8 (the 384 kHz ← 48 kHz factor), or `compile` rejects it.
fn chassis(preset: u8, block: usize) -> (Schedule, EventInputId) {
    let mut g = Graph::new();
    let src = g.add(SoundFontSource::new(&soundfont_bytes(), preset).expect("valid GM soundfont"));
    let da = g.add(DaConverter::new(
        SampleRate::new(48_000.0),
        BitDepth::new(24),
        Volts::new(1.0),
        Ohms::new(150.0),
    ));
    g.connect_ideal(src, 0, da, 0); // the digital edge → a DigitalRoute, 8× to the analog rate
    g.set_output(da, 0); // the schedule output tap is the DA's analog volts
    let sched = compile(g, block, rate(), 0).expect("valid chassis");
    let ev = sched.event_input(src, 0).expect("open event input");
    (sched, ev)
}

/// Play a single note-on at `offset` (and an optional note-off), render one `block`, return the volts.
fn play(preset: u8, block: usize, note: u8, on_at: u64, off_at: Option<u64>) -> Vec<f32> {
    let (mut sched, ev) = chassis(preset, block);
    let mut q = EventQueue::with_capacity(4);
    q.push(
        on_at,
        ev,
        EventMessage::NoteOn {
            note,
            velocity: 100,
        },
    );
    if let Some(off) = off_at {
        q.push(off, ev, EventMessage::NoteOff { note });
    }
    let mut out = VoltageBuffer::zeros(block, rate());
    sched.process_with_events(&mut out, &mut q);
    out.as_slice().to_vec()
}

#[test]
fn chassis_is_silent_until_triggered() {
    // No events ⇒ the DA reconstructs digital silence to exactly 0 V.
    let (mut sched, _ev) = chassis(0, 8_192);
    let mut out = VoltageBuffer::zeros(8_192, rate());
    sched.process(&mut out);
    assert!(
        out.as_slice().iter().all(|&v| v == 0.0),
        "an untriggered chassis must drive exact silence"
    );
}

#[test]
fn chassis_plays_a_note_as_finite_volts() {
    // A played piano note comes out as finite, audible volts, within a sane band of the 1 V reference.
    let out = play(0, 32_768, 60, 0, None);
    assert!(out.iter().all(|v| v.is_finite()), "volts must be finite");
    let peak = out.iter().fold(0.0_f32, |m, &v| m.max(v.abs()));
    assert!(peak > 1e-3, "the note should be audible, got peak {peak} V");
    assert!(
        peak < 4.0,
        "a 1 V-reference chassis shouldn't emit {peak} V"
    );
}

#[test]
fn chassis_plays_in_tune() {
    // A4 (note 69 = 440 Hz) on the sustained organ: the 440 Hz fundamental dominates a detuned,
    // non-harmonic bin (550 Hz) by a wide margin — the chassis plays at the right pitch.
    let block = 32_768;
    let out = play(GM_ORGAN, block, 69, 0, None);
    // Skip the attack; measure the steady sustain.
    let tail = &out[block / 4..];
    let at_pitch = tone_amplitude(tail, 440.0, rate());
    let off_pitch = tone_amplitude(tail, 550.0, rate());
    assert!(
        at_pitch > off_pitch * 5.0,
        "the 440 Hz fundamental ({at_pitch}) should dominate a detuned bin ({off_pitch})"
    );
    assert!(at_pitch > 1e-3, "the note should be clearly audible");
}

#[test]
fn chassis_honors_note_timing() {
    // A note-on at the block midpoint: the output is silent well *before* it and sounds *after*.
    // Not a sample-exact boundary — the DA's linear-phase group delay (~80 analog samples for 161
    // taps) plus rustysynth's block_size quantization (64 frames · 8 = 512 analog) smear it, so the
    // assertions leave a guard band around the trigger rather than checking the exact sample.
    let block = 32_768;
    let on_at = block as u64 / 2;
    let out = play(GM_ORGAN, block, 69, on_at, None);

    let guard = 512 + 160; // block_size quantization + DA group delay, with margin
    let before = &out[..on_at as usize - guard];
    let after = &out[on_at as usize + guard..];
    assert!(
        rms(before) < 1e-4,
        "must be silent before the trigger, rms {}",
        rms(before)
    );
    assert!(
        rms(after) > 1e-3,
        "the note must sound after the trigger, rms {}",
        rms(after)
    );
}

#[test]
fn chassis_note_off_releases_to_near_silence() {
    // Organ (sustains while held) note-on at 0, released a quarter of the way in: with reverb/chorus
    // off there's no lingering tail, so the final stretch settles back toward silence.
    let block = 65_536;
    let out = play(GM_ORGAN, block, 69, 0, Some(block as u64 / 4));
    let tail = &out[block - 4_096..];
    assert!(
        rms(tail) < 1e-3,
        "a released note should decay to near silence, tail rms {}",
        rms(tail)
    );
}

// ---- Local measurement helpers (ported from the crate-internal `test_util`) --------------------

/// Root-mean-square of a slice (empty → 0).
fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|&x| f64::from(x) * f64::from(x)).sum();
    (sum_sq / samples.len() as f64).sqrt() as f32
}

/// Peak amplitude of the `freq_hz` component of `samples`, by a single-bin DFT (empty → 0).
fn tone_amplitude(samples: &[f32], freq_hz: f64, rate: AnalogRate) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let dt = rate.seconds_per_sample();
    let omega = std::f64::consts::TAU * freq_hz;
    let (mut re, mut im) = (0.0_f64, 0.0_f64);
    for (n, &x) in samples.iter().enumerate() {
        let phase = omega * (n as f64 * dt);
        re += f64::from(x) * phase.cos();
        im += f64::from(x) * phase.sin();
    }
    let n = samples.len() as f64;
    (2.0 / n * (re * re + im * im).sqrt()) as f32
}
