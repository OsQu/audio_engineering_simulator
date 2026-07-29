# Story 9.2 — The `SoundFontSource` node + internal-DA chassis (headless) — program design

> Companion to the Story 9.2 block in `docs/IMPLEMENTATION_PLAN.md`. Holds the per-Task program
> design — the shape of the code — drafted just-in-time as each Task is picked up.

Chassis at a glance (headless, tested in-graph):

```
events ──▶ SoundFontSource ──(Sample @ 48 kHz, mono)──▶ DaConverter ──▶ analog volts (384 kHz)
             │  rustysynth render → 0.5·(L+R) → clamp ±1        │  existing 8× Interpolator + de-cal
             └─ DigitalFace out (M = 8)                         └─ DigitalRoute edge feeds it
```

## Task 9.2.1 — `rustysynth` dep + font asset + the `SoundFontSource` node  · done

**File-tree diff:**

```
 Cargo.lock                                   (+ rustysynth 1.3.6 and its deps)
 crates/engine/Cargo.toml                     (+ rustysynth = "1.3.6")
 crates/engine/src/node.rs                    (+ mod sound_font; + pub use SoundFontSource)
 crates/engine/src/lib.rs                     (+ SoundFontSource in the re-export list)
+crates/engine/src/node/sound_font.rs         the node + its clamp/parse wrapper
+crates/engine/tests/soundfont.rs             native node-only test (reads the shared asset)
+assets/soundfonts/TimGM6mb.sf2               ~6 MB GM font, shared with the 9.4 web build
```

**Key types & signatures:**

```rust
// node/sound_font.rs — rustysynth is fully internal; no rustysynth type is `pub`.
use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};

/// Construct-time failure (font parse / synth build). Returned by `new`, never on the hot path.
#[derive(Debug)]
pub enum SoundFontError { Parse, Build }   // wraps rustysynth's SanityCheckFailed etc. (no rustysynth type leaks)

pub struct SoundFontSource {
    synth: Synthesizer,          // owns its Arc<SoundFont>; voice pool pre-allocated at construction
    preset: u8,                  // GM program, fixed in 9.2 (selection is 9.3)
    m: usize,                    // analog/digital factor, baked in prepare(); frame = offset / m
    inputs:  [InputPort; 1],     // [EventFace::new(EVENT_CAPACITY)]
    outputs: [OutputPort; 1],    // [DigitalFace::new(AudioFormat::new(48 kHz, bits, 1))]
}

impl SoundFontSource {
    const RATE_HZ: i32 = 48_000; // pinned so M = 8 exactly (no fractional resampler)
    const CHUNK: usize = 64;     // == rustysynth default block_size; stack scratch length

    /// Parse the font and build the synth. Fallible + allocates — construction only, never `process`.
    pub fn new(sf2_bytes: &[u8], preset: u8) -> Result<Self, SoundFontError>;

    /// Clamp to valid MIDI and forward to rustysynth (channel 0). Total, no panic.
    fn note_on(&mut self, key: u8, vel: u8);   // key/vel already u8 (0..=127) — clamp is belt-and-suspenders
    fn note_off(&mut self, key: u8);

    /// Render `n` frames into the output slice starting at `at`, folding L+R→mono, clamped ±1.
    /// Uses `[f32; CHUNK]` stack scratch, looping in ≤CHUNK pieces — zero heap.
    fn render_into(&mut self, out: &mut [f32], at: usize, n: usize);
}
```

**`process` call-stack (the event-sub-slice loop):**

```
SoundFontSource::process(params, inputs, outputs)
  let events = inputs[0].events().as_slice()        // TimedEvent { offset (analog), message }
  let out    = outputs[0].sample_mut().as_mut_slice()   // len = block / M frames @ 48 kHz
  cursor = 0
  for ev in events:                                 // already time-ordered
      f = min(ev.offset / m, out.len())             // analog offset → digital frame
      render_into(out, cursor, f - cursor)          //   render up to the event
        └─ loop: synth.render(&mut lbuf[..k], &mut rbuf[..k]); out[j] = 0.5*(l+r) clamped ±1
      match ev.message:                             //   then apply it
          NoteOn { note, velocity } => note_on(note, velocity)
          NoteOff { note }          => note_off(note)
          Gate(_)                   => {}           // ignored: a bare gate has no pitch
      cursor = f
  render_into(out, cursor, out.len() - cursor)      // tail after the last event
```

**Node trait wiring:** `prepare(rate)` bakes `m = round(rate / 48 kHz)` (validated by `compile`'s
DigitalFace lane-sizing, same as the AD). `params()` → `&[]`. `seed()` → no-op (deterministic).
No `group_delay_samples` override (the DA reports the chassis latency).

**Done:** the node renders a recognizable middle-C GM note onto its `Sample` lane (finite, non-silent);
garbage bytes surface `SoundFontError` at construction, not a panic.

## Task 9.2.2 — The headless chassis end-to-end (→ DA → volts)  · done

No new `src`: the chassis is wired **in the test** (the device/catalog grouping is 9.4). Tests live in
`crates/engine/tests/soundfont.rs` alongside 9.2.1 (the shared-asset runtime-read pattern; `test_util` is
`#[cfg(test)]`-internal and unreachable from an integration test, so small local `rms`/`tone_amplitude`
helpers are ported in — a single-bin DFT, ~15 lines).

**Chassis wiring + helper (key signatures):**

```rust
// A compiled SoundFontSource → DaConverter chassis + its open event input.
fn chassis(preset: u8, block: usize) -> (Schedule, EventInputId) {
    let mut g = Graph::new();
    let src = g.add(SoundFontSource::new(&soundfont_bytes(), preset).expect("valid GM soundfont"));
    let da  = g.add(DaConverter::new(SampleRate::new(48_000.0), BitDepth::new(24),
                                     Volts::new(1.0), Ohms::new(150.0)));  // 1.0 V ref: full scale → ±1 V
    g.connect_ideal(src, 0, da, 0);   // the digital edge → a DigitalRoute (8× to 384 kHz)
    g.set_output(da, 0);              // schedule output is analog volts
    let sched = compile(g, block, rate(), 0).expect("valid chassis");
    let ev = sched.event_input(src, 0).expect("open event input");
    (sched, ev)
}
// drive: EventQueue::with_capacity(n).push(offset, ev, NoteOn{..}); sched.process_with_events(&mut volts, &mut q)
```

**Test plan & the fuzz the DA introduces:**
- `block` a multiple of `M = 8` (compile rejects otherwise); a large-ish block (e.g. 32 768 analog =
  4096 digital frames ≈ 85 ms) so a note clearly sounds.
- **silent-until-triggered:** no events ⇒ DA of digital silence is exact 0 V.
- **finite, non-silent volts** on a played note; peak within a sane band (the 1 V reference ⇒ ≈±1 V).
- **in-tune:** play A4 (note 69). Use a **sustained** preset (drawbar organ, GM **16**) so the tone is
  steady for the DFT (piano decays); assert `tone_amplitude(tail, 440) ≫ tone_amplitude(tail, 550)` (a
  non-harmonic bin), mirroring `synth.rs`'s in-tune check.
- **timing honored (not sample-exact):** note-on at analog `block/2`; the region *well before* it is
  silent and *after* it sounds — with a **guard band** for the DA's linear-phase group delay (~80 analog
  samples for 161 taps) **plus** rustysynth's `block_size` quantization (64 frames · M = 512 analog). Not
  a sample-exact boundary (the DA smears it) — documented, not a bug.
- **note-off releases:** organ note-on at 0, note-off at `block/4`; the final tail's `rms` falls to near
  silence (reverb/chorus off ⇒ no lingering tail).

**Done:** the headless chassis turns note events into recognizable, correctly-pitched volts; gating +
release verified.

## Task 9.2.3 — Polyphony + velocity  · planned
_(program design drafted at pickup)_

## Task 9.2.4 — Hot-path gates on the node's `process`  · planned
_(program design drafted at pickup — extends `crates/engine/tests/no_alloc.rs` with the chassis; the
counting allocator asserts zero over ≥128 event-driven blocks + a voice-steal burst.)_
