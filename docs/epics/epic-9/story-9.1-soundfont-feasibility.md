# Story 9.1 — SoundFont hot-path feasibility spike — delivery record

`rustysynth` v1.3.6 (MIT) with the vendored TimGM6mb GM SoundFont passed the Epic 9 feasibility gate.
The spike was deliberately throwaway; its harnesses were removed after the verdict and the durable findings
are recorded here.

## Verdict: GO

- A recognizable middle-C GM note rendered from SoundFont bytes, finite and non-silent.
- The render path made zero allocations, including its first render, across 128 blocks with a held C–E–G
  chord and an 80-note voice-stealing burst beyond the default 64-voice pool.
- Adversarial note-on/off values and exhausted polyphony did not panic on the note/render path.
- The dependency and byte-backed `Cursor` load compiled for `wasm32-unknown-unknown`; no filesystem API,
  fork, or OxiSynth fallback was needed.

## Decisions carried into later stories

- TimGM6mb is the v1 font: the smaller VintageDreamsWaves demo font failed `rustysynth`'s structural
  SoundFont sanity check. A future font remains a compatibility choice that must be parsed before adoption.
- SoundFont playback is deterministic. The render path does not depend on time, threads, or ambient RNG.
- V1 drives notes only. Program choice happens at construction rather than as an event-lane program change;
  future pitch-bend, sustain, or program-change work belongs to the optional Story 9.5 scope.

The production `SoundFontSource` arrived in Story 9.2, shared-bank asset delivery in Story 9.3, and the
playable guitar devices in Story 9.4.
