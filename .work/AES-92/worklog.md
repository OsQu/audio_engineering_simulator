# AES-92 worklog

## 2026-11-08
**Did**
- Slice 1 (9ea8115): `benchScene` adds the `spk` instance when the default tap falls back to the speaker. New test asserts `output.device ∈ patch.devices` for a digital-only DUT.
- Slice 2 (b7c974a): `EngineControl.close` → `dispose` (port onmessage cleared + closed, graph disconnected, context closed; failed bring-up closes its context). `wireMidi` returns a detach that clears only its own handlers. `SceneSession.wireMidi` / `dispose` (engine, storage worker terminate, MIDI, topology timer, dispose-during-start). App + Workbench dispose from an effect cleanup.
- Slice 3 (5e55021): `Describe` resolver + `typeDescriber` in catalog.ts; params / patching / bench-watch / scene-ops / DebugPanel take it; session passes `descriptorOf` (made an arrow property) and reseeds params when `deviceDescriptors` lands. New `test/per-instance-descriptors.test.ts` uses an 8-send × 4-track computer.

**Verified**
- `pnpm run check`, `typecheck` (only the 2 pre-existing FloatingWindow a11y warnings), `test` (429), and `test:browser` (40/40) green.
- Manual (Playwright, real worklet): studio ▶ start → `/devices/computer` → back, 5×, with an `AudioContext` constructor counter: exactly 1 live context at every step, all previous ones `closed`; 0 live after returning to the un-started studio. Computer bench loads with "Speaker · Tap" as the tap.

**Learned**
- Workbench's start effect re-runs on reactive reads inside `start()` (`plainPatch()` snapshots the scene), so the dispose cleanup must sit in its own dependency-free effect — putting it in the start effect's cleanup would dispose the session on every scene edit.
- Per-instance descriptors arrive with the post-install `deviceDescriptors` message, *after* the topology accept that reseeds params.
- `test:browser` refuses to run if a dev server already holds `$VITE_PORT` (no reuseExistingServer).

**Next**: done — PR draft written.
