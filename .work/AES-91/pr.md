Validate each device against its own descriptor; keep scene frees and record drains off the render call

## What changed and why

**1. `build_patch` checked connections against the wrong descriptor.** The descriptor map was keyed by
*type id*, so with two `computer`s on different `usb_sends`/`usb_returns` configs, the last instance's
face was used for both. That broke the channel-count, connector, duplex and output-tap checks: they
could wrongly reject or accept a patch. Every device was also built twice (once to instantiate, once
to describe), plus a third time in the worklet on each `loadPatch`.

- `catalog::instantiate_described` returns `(BuiltDevice, DeviceDescriptor)` from **one** expansion:
  `describe` is split so the descriptor comes from the face of the nodes actually built.
- `build_patch` keys descriptors by device id and keeps them on `BuiltScene::device_descriptors()`.
- `SceneEngine::device_descriptors()` / `pending_device_descriptors()` hand them to the worklet as a
  plain object keyed by device id. The worklet no longer calls `describe_device`, so that wasm export
  is removed.

**2. The old scene was freed on the audio thread.** `render_quantum` (called from the worklet's
`process()`) assigned over the live scene, which freed its schedule, nodes and FIR kernels there. The
consumed initial-param `Vec` was freed there too. The installing quantum now swaps the scenes in place
and parks the old one in a `retired` slot. `load_patch` and the control setters free it later, from
message handlers.

**3. The record drain allocated per call.** `DawControl::drain_record -> Vec<u8>` is replaced by
`drain_record_into(track, &mut [u8]) -> usize`. `SceneEngine` drains into a scratch buffer sized to the
record ring, and the worklet copies the bytes out through `record_ptr()`.

`ScheduleSlot` had no production users, so it is removed.

## Tests
- `devices`: `each_computer_is_validated_against_its_own_config` and
  `a_narrow_computer_rejects_the_wide_send_regardless_of_order`. Both fail on the old code.
- `wasm-bindings/tests/no_alloc.rs`: a counting allocator that counts both allocations **and frees**.
  It shows that the installing `render_quantum`, the record drain and steady quanta are heap-silent,
  and that the next control call does free the retired scene. On the old code the installing quantum
  freed 1053 blocks.
- `engine`: `drain_ring_into` frames-only / partial-drain unit test.
- Browser: the worklet smoke spec now asserts that descriptors arrive keyed by every device id at
  `ready` and after an installed hot-swap.

## Look closely at
- Where the retired scene is freed (`release_retired` call sites in `crates/wasm-bindings/src/lib.rs`).
  It is freed only from calls the worklet makes in `onmessage`, never from `render_quantum` or the
  getters and drain called in `process()`. `queue_patch` empties `retired` before setting `pending`, so
  an install never overwrites (frees) a previous retiree.
- `web/worklet/processor-impl.js`: the new drain/copy-out and descriptor sourcing.
- The WASM artifact must be rebuilt (`pnpm run wasm`) to see the worklet changes in the browser.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
