# AES-72 worklog

## 2026-11-08
**Did**
- Rebuilt WASM, ran Vite on $VITE_PORT, and drove the studio with the Playwright MCP.
- Seeded a scene: the default loop plus an unpatched, powered Luma, the 8i6 turned to its rear panel, and crosspoint 130 (DAW 1 → Line Out 3).
- Patched `if:output:4 → luma:input:0` and `luma:output:0 → if:input:2` with jack clicks. Both were accepted with the `balanced_trs_xlr_3m` adapter. After ⚙ Scene → save and a reload, both cables were restored.
- Audio check: while a note played, send 1 read 0.0 dBFS and send 3 read about −1.3 dBFS. Send 3 went silent on note release. With the Luma powered off, send 3 went silent while send 1 stayed. 0 overruns.
- Added `luma_hardware_insert_builds_and_passes_signal_through_the_luma` (devices/build.rs). It checks that the insert builds, the cable losses match hand calcs (−1.940 / −0.118 dB), and Line Out 4 carries signal only while the Luma is powered.
- Closed the bug doc with an Acceptance section and marked the IMPROVEMENTS item superseded.
- Gates: fmt, lint, test, wasm and docs are green. `worklet-patch-acceptance.spec.ts` passes (3/3, single worker). No web source changed.

**Learned**
- The 8i6 Power param (205) fans out to many node handles. A `ParamQueue::with_capacity(16)` silently drops the overflow, which gave a confusing 0 V. Use 64.
- In the studio, playing a note means opening the controller's focus window ("open MIDI Controller") and then holding a QWERTY key. Double-clicking the device doesn't do it.
- A Playwright `click({force:true})` on the Luma's Power switch didn't always toggle it. `el.click()` via evaluate did.

**Next**
- PR draft, push, fx mv finished.
