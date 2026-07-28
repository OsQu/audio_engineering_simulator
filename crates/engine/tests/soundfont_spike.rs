//! THROWAWAY SPIKE (Story 9.1) — feasibility of `rustysynth` as the black-box synth core.
//!
//! Answers the go/no-go gates before we build anything on it:
//!  - 9.1.1 plays a recognizable GM note (non-silent, finite output)
//!  - 9.1.2 the ZERO-ALLOC gate: is `render()` allocation-free in steady state?
//!  - 9.1.3 quick panic probes on note_on/render with adversarial input
//!
//! Loads the SoundFont from a `Cursor<&[u8]>` (NOT `File`) — the wasm path.
//! Delete this whole file (and tests/spike.sf2) when the real node lands in 9.2.
#![allow(unsafe_code, reason = "spike: GlobalAlloc to observe allocations, like tests/no_alloc.rs")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::io::Cursor;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};

/// Counts allocations only while `MEASURING` is true; always forwards to System.
struct CountingAlloc;
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static MEASURING: AtomicBool = AtomicBool::new(false);

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if MEASURING.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

const SF2: &[u8] = include_bytes!("spike.sf2");

/// One engine digital block: 1024 analog samples / M=8 = 128 frames @ 48 kHz.
const BLOCK: usize = 128;

fn make_synth() -> Synthesizer {
    // NOT File — a Cursor over bytes, exactly how the wasm host will feed the font.
    let mut cur = Cursor::new(SF2);
    let sf = Arc::new(SoundFont::new(&mut cur).expect("valid soundfont"));
    let settings = SynthesizerSettings::new(48_000); // pin to 48k (→ 8× to the 384k analog rate)
    Synthesizer::new(&sf, &settings).expect("synth construct")
}

#[test]
fn spike_verdict() {
    // ---- 9.1.1: plays a recognizable note ---------------------------------
    let mut synth = make_synth();
    eprintln!(
        "polyphony={}, block_size={}, sample_rate={}, reverb/chorus={}",
        synth.get_maximum_polyphony(),
        synth.get_block_size(),
        synth.get_sample_rate(),
        synth.get_enable_reverb_and_chorus(),
    );
    synth.note_on(0, 60, 100); // middle C, GM preset 0 (piano-ish)
    let mut l = vec![0f32; BLOCK];
    let mut r = vec![0f32; BLOCK];
    // render a bunch so the note has clearly sounded
    let mut peak = 0f32;
    for _ in 0..64 {
        synth.render(&mut l, &mut r);
        for &s in &l {
            assert!(s.is_finite(), "non-finite sample");
            peak = peak.max(s.abs());
        }
    }
    eprintln!("9.1.1 peak over 64 blocks = {peak:.4}");
    assert!(peak > 1e-4, "expected audible output, got silence (peak {peak})");

    // ---- 9.1.2: ZERO-ALLOC gate -------------------------------------------
    // Hold a chord, measure allocations across many render blocks. Report both
    // "including first render" and "steady state after a warmup", because a
    // one-time lazy alloc on first render is materially different from per-block.
    let mut synth = make_synth();
    synth.note_on(0, 60, 100);
    synth.note_on(0, 64, 100);
    synth.note_on(0, 67, 100); // C-E-G held

    // (a) including the very first render call
    ALLOCS.store(0, Ordering::Relaxed);
    MEASURING.store(true, Ordering::Relaxed);
    synth.render(&mut l, &mut r);
    let first = ALLOCS.load(Ordering::Relaxed);
    for _ in 0..128 {
        synth.render(&mut l, &mut r);
    }
    MEASURING.store(false, Ordering::Relaxed);
    let with_first = ALLOCS.load(Ordering::Relaxed);
    eprintln!(
        "9.1.2 allocs: first render = {first}, +128 steady blocks (chord held) = {}",
        with_first - first
    );

    // (b) voice-stealing burst: fire more note-ons than max polyphony, keep rendering
    let poly = synth.get_maximum_polyphony();
    ALLOCS.store(0, Ordering::Relaxed);
    MEASURING.store(true, Ordering::Relaxed);
    for k in 0..(poly + 16) {
        synth.note_on(0, 36 + (k % 60) as i32, 100);
        synth.render(&mut l, &mut r);
    }
    MEASURING.store(false, Ordering::Relaxed);
    let steal = ALLOCS.load(Ordering::Relaxed);
    eprintln!("9.1.2 allocs during voice-steal burst ({} note_ons) = {steal}", poly + 16);

    // ---- 9.1.3: quick panic probes ----------------------------------------
    // Adversarial input on the note path — do these panic?
    let mut synth = make_synth();
    synth.note_on(99, 200, 300); // out-of-range channel/key/velocity
    synth.note_on(0, -5, -1);
    synth.note_off(50, 999);
    synth.render(&mut l, &mut r);
    for &s in &l {
        assert!(s.is_finite());
    }
    eprintln!("9.1.3 adversarial note_on/off/render did NOT panic");

    // The verdict is in stderr; run with `--nocapture`. Assert only the non-negotiable:
    // per-block steady-state allocation must be zero.
    assert_eq!(
        with_first - first,
        0,
        "STEADY-STATE render allocated {} times over 128 blocks — NO-GO as-is",
        with_first - first
    );
}
