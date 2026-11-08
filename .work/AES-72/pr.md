Accept the Scarlett ↔ Luma hardware insert in the browser; close the bug

## What
The Scarlett ↔ Luma hardware-insert patch was wrongly rejected as a feedback loop. The transactional, engine-validated patch path landed earlier; this PR records the browser and audio acceptance run and closes `docs/bugs/2026-09-09-luma_loop.md`.

- **Browser acceptance (live dev server, real AudioWorklet, Playwright).** Both cables (Line Out 3 → Luma Line In, Luma Line Out → Line In 3) were patched with the normal jack gesture and accepted. Both survive save and a page reload. With the insert routed, USB send 3 carries the Luma's return at about −1.3 dBFS while a note plays. It goes silent on note release, and also goes silent when the Luma is powered off. 0 overruns. Details are in the bug doc's new *Acceptance* section.
- **New regression test** `luma_hardware_insert_builds_and_passes_signal_through_the_luma` in `crates/devices/src/build.rs`:
  - The two-cable insert builds with no device-specific exception; its only breakable edges are digital, inside the interface.
  - Both insert cables keep their analog loading, with hand-calculated losses: −1.940 dB (150 Ω DA → 600 Ω) and −0.118 dB (600 Ω → 44 kΩ receiver).
  - A note routed Pre 1 → Line Out 3 → Luma → Line In 3 → Line Out 4 arrives only while the Luma is powered.
- `docs/IMPROVEMENTS.md`: marked the "domain-aware `wouldCreateCycle`" item superseded, because the coarse check no longer decides connections.

## Look closely at
- The test's param queue is sized to 64 because the 8i6 Power param fans out to many handles; a smaller queue silently drops the power-on.
- The peak is measured only over the second half of the run, so the Luma's 5 ms power-off glide doesn't count as leakage.

No web or engine source changed. WASM doesn't need a rebuild.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
