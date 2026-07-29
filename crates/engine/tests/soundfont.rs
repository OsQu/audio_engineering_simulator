//! Story 9.2 — the `SoundFontSource` node.
//!
//! Task 9.2.1 drives the node **alone** (no schedule): parse a real GM font, play a note through
//! `process`, and confirm a recognizable, finite, non-silent signal lands on the digital `Sample`
//! lane. The full chassis (→ `DaConverter` → volts) and the hot-path gates are later tasks.

use engine::{
    AnalogRate, BitDepth, ClockDomainId, EventBuffer, EventMessage, Lane, Node, Params,
    SampleBuffer, SampleRate, SoundFontError, SoundFontSource, TimedEvent,
};

/// The analog rate the engine runs at; 384 kHz / 48 kHz = 8× to the synth's digital rate.
fn rate() -> AnalogRate {
    AnalogRate::new(384_000.0)
}

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
