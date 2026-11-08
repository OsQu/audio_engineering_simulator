Web fixes: bench tap for digital-only devices, engine teardown on view switch, per-instance descriptors

Three independent web correctness fixes from the project audit, one commit each.

## 1. The bench tapped a device that wasn't in the patch
For a device with no analog output (the computer), `defaultBenchTap` falls back to the speaker's tap, but `benchScene` only put the device under test in `patch.devices`, so `output` named a missing device. `benchScene` now adds the `spk` instance whenever the tap falls back to it. I picked this over "return `undefined` + show a hint" because it keeps the computer benchable (add a DA, patch through to the speaker) and needs no new UI.

## 2. View switches leaked the audio engine
Every scene ↔ `/devices/x` switch left an `AudioContext` + worklet + OPFS worker running, and Web MIDI stayed bound to the dead view.
- `startEngine`'s handle: `close()` → `dispose()`. It clears the port handler, closes the port, disconnects the graph and closes the context. A failed bring-up now closes its context before rethrowing.
- `wireMidi` returns a detach. It clears only the handlers it bound, because MIDI inputs can be shared with the next view's wiring.
- `SceneSession.wireMidi(onNote)` and `SceneSession.dispose()`. Dispose releases the engine, terminates the storage worker, unbinds MIDI and clears the topology timer. If the view unmounts while `start()` is still in flight, the engine is torn down as soon as it lands.
- `App` and `Workbench` call `session.dispose()` from an effect cleanup.

**Look closely:** in `Workbench.svelte` the dispose effect is a separate, dependency-free `$effect`. The existing start effect re-runs on reactive reads inside `start()` (the patch snapshot), so a cleanup there would dispose the session on every scene edit.

**Manual check:** with a counter on the `AudioContext` constructor, I ran studio ▶ start → workbench → back 5× in the real app. Exactly one context was live at every step and all the earlier ones were `closed`.

## 3. Pure helpers used type descriptors, not per-instance ones
A config-driven computer has more crosspoint params and wider ports than its type's default face.
- New `Describe = (deviceId) => DeviceDescriptor | undefined` type in `catalog.ts`, plus `typeDescriber(scene, catalog)` for type-only contexts such as tests.
- These now take a `Describe`, and the session passes `descriptorOf` (now an arrow property so it can be passed around):
  - `params.seedParamValues` / `pushParams`
  - `patching.endpointFromJackKey`
  - `bench-watch.watchables`
  - `scene-ops` connection helpers (`connectionDomain` / `Kind` / `Connectors`, `cablesFor`, `commitCable`, `enumerateComputerUsb`)
  - `DebugPanel`
- Helpers that only used `scene` to look up a device's type dropped that parameter.
- The session also reseeds param values when the `deviceDescriptors` message arrives. That message comes after a hot-swap installs, which is later than the accept-time seed, so a config change that grows the crossbar would otherwise leave the new params unseeded.

## Tests
- `workbench-scene.test.ts`: a digital-only DUT's `output.device` is in `patch.devices`.
- New `per-instance-descriptors.test.ts`: an 8-send × 4-track computer, checking seed, push, watchables, jack resolution and connectors against the configured face.
- `pnpm run check`, `typecheck`, `test` (429) and `test:browser` (40/40) all pass. No Rust or WASM changes, so no rebuild is needed.

## Not included
The race between `hotSwap()` and a pending candidate belongs to the acknowledged-topology-transactions work.
