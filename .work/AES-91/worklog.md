# AES-91 worklog

## 2026-11-08
**Did**
- `a227a2b` build_patch: `instantiate_described` returns `(BuiltDevice, DeviceDescriptor)` from one
  expansion (`describe` split into `describe_face`); `descs` keyed by device id;
  `BuiltScene::device_descriptors()`. Two regression tests (two computers, both orders). I checked that
  both fail on the old code: one is a wrongly rejected `ChannelCountMismatch`, the other gets the
  engine's `LaneCountMismatch` instead of the legible error.
- `432468c` SceneEngine `device_descriptors()` / `pending_device_descriptors()` (maps → plain objects);
  worklet no longer re-describes; `describe_device` wasm export removed.
- `359428b` retired-scene slot (swap in place; old scene + spent initial list parked; freed by
  load_patch/control setters); `DawControl::drain_record_into` + SceneEngine record scratch +
  `record_ptr`; counting-allocator test `crates/wasm-bindings/tests/no_alloc.rs`. On the old code
  the installing quantum showed 1053 frees.
- `5331ad4` removed `ScheduleSlot`.
- `4fbb435` worklet smoke spec asserts ready + installed descriptors keyed by every device id.

**Learned**
- `Pending.initial` was a Vec consumed (freed) in render_quantum as well as the scene. Parking the
  whole `Pending` covers both.
- serde_wasm_bindgen serializes BTreeMap as a JS `Map` by default; I used `serialize_maps_as_objects`.

**Result**: all gates green (fmt, lint, test, wasm, docs; web format/check/typecheck/vitest); full
Playwright suite 40/40 against a rebuilt wasm.
