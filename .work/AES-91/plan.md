# AES-91 — build_patch descriptor keying + audio-thread deallocation on scene swap

## Intent
1. `build_patch` validated connections against a descriptor keyed by **type id** (last instance of a
   type wins), and described every device twice (+ a third time in the worklet). Validate each device
   against its *own* per-instance descriptor, built from the same expansion that instantiates it, and
   ship those descriptors to the worklet from the built scene.
2. `render_quantum` dropped the old scene (schedule, FIR kernels, …) on the audio thread. Retire it
   instead and free it from the next off-render call. Same for the record drain's per-call `Vec`.

## Approach / slices
| # | Slice | Status |
|---|-------|--------|
| 1 | devices: `instantiate_described` → `(BuiltDevice, DeviceDescriptor)` from one expansion; `descs` keyed by device id; `BuiltScene::device_descriptors()`; two-computer regression test | done |
| 2 | wasm-bindings + worklet: `SceneEngine::device_descriptors()` / `pending_device_descriptors()`; worklet stops calling `describe_device` | done |
| 3 | wasm-bindings: `retired` slot (old scene + consumed initial-param Vec), freed in `load_patch`/control methods; counting-allocator integration test (no alloc/dealloc in the installing `render_quantum`) | done |
| 4 | engine: `DawControl::drain_record_into(track, &mut [u8]) -> usize`; SceneEngine scratch; worklet reads scratch via ptr; no-alloc test covers the drain | done |
| 5 | delete unused `ScheduleSlot` | done |
| 6 | PR draft + worklog | done |

## Decisions
- `describe(entry, config)` split into `describe_face(entry, &FaceExpansion)` so instantiate reuses the
  face of the nodes it actually built (one expansion, no second node build / FIR design).
- Retired scene is freed by `load_patch` and the *mutating* control methods (set_param, notes,
  transport, track setters, feed_playback) — not by the getters/drain the worklet calls from
  `process()`. In an AudioWorklet `onmessage` shares the audio thread, but runs outside the render
  callback, which is the boundary the hot-path rule (and the existing "off-block" model) draws.
- The `Pending` struct is reused as the retired holder (swap scenes in place) so the consumed
  `initial` param Vec is not freed on the installing call either.
- Descriptor map crosses to JS as a plain object (`serialize_maps_as_objects`), matching the
  `Record<string, DeviceDescriptor>` the UI already consumes.
- Record scratch sized to the recorder's per-track ring (`MultitrackRecorder::STREAM_RING_BYTES`, made
  public) so one drain empties the ring.

## Open questions
- none

Note: slices 3 and 4 landed as one commit (they share the SceneEngine fields and the one no-alloc
test). Extra: removed the unused `describe_device` wasm export (assumption I-281) and added a
browser-smoke assertion that descriptors arrive keyed by device id.
