# AES-92 — web fixes: bench tap, engine teardown, per-instance descriptors

## Intent
Three independent web correctness fixes from the project audit.

## Approach / slices
| # | Slice | Status |
|---|-------|--------|
| 1 | `benchScene` adds the speaker instance when the tap falls back to it (digital-only DUT) + test | done |
| 2 | Engine teardown: `startEngine` → `dispose()` (port close, node disconnect, `audio.close()`, and close on failed bring-up); `SceneSession.dispose()` (engine, storage worker, MIDI unbind, topology timer); `$effect` cleanup in `App` / `Workbench` | done |
| 3 | Per-instance descriptor resolver (`Describe = (deviceId) => DeviceDescriptor \| undefined`) threaded through params / patching / bench-watch / scene-ops / DebugPanel; session passes `descriptorOf` + reseeds params when per-instance descriptors land | done |

## Decisions
- (1) Add the speaker rather than return `undefined`: keeps a digital-only device benchable (patch it via an added DA → the speaker) and needs no new Workbench hint UI.
- (2) Rename `EngineControl.close` → `dispose` (one teardown verb across engine + session); `WorkletSmokeRoot` updated.
- (2) MIDI wiring moves onto the session (`session.wireMidi(onNote)`) so the session owns the unbind — `wireMidi` returns a detach that clears `onmidimessage`/`onstatechange` (and cancels a pending access request).
- (2) `dispose()` during an in-flight `start()` closes the engine as soon as `startEngine` resolves.
- (3) Session reseeds `paramValues` when the `deviceDescriptors` message lands (it arrives after install, so a config change that grows the computer's crossbar would otherwise leave the new params unseeded).

## Out of scope
- `hotSwap()` vs pending candidate race — owned by acknowledged topology transactions work.
