# AES-72 — Luma hardware-insert loop: browser/audio acceptance

## Intent
Run the outstanding acceptance steps from `docs/bugs/2026-09-09-luma_loop.md` (criteria 1–3 in the live
browser: two-cable Scarlett↔Luma insert accepted, survives save/reload, audio passes through the Luma),
then mark the bug doc closed.

## Approach
- Start Vite on `$VITE_PORT`, drive the studio view with the Playwright MCP.
- Place a Luma in the studio (seeded scene), patch the two cables via the real UI patch path.
- Reload: both cables still present (localStorage scene).
- Route the 8i6 matrix as a hardware insert; check via Debug meters that signal reaches the Luma and
  returns on the USB send.
- Record evidence in the bug doc's Resolution, set Status closed.

## Slices
| # | Slice | Status |
|---|---|---|
| 1 | Plan + server up | done |
| 2 | Criterion 1: insert accepted + survives reload | done |
| 3 | Criteria 2–3: audible through Luma, cycle break on digital edge | done |
| 4 | Close bug doc, PR draft | done |

## Decisions
- Seed the studio scene via localStorage (default scene + an unpatched Luma + one 8i6 crosspoint DAW 1 → Line Out 3), but make both insert cables with the real jack click-click gesture, which goes through the transactional patch path under test.
- Add a `build_patch` regression test for the two-cable insert. No existing test built it, and criterion 3 (digital cycle break, analog cables intact) is easiest to pin numerically.
- Mark the IMPROVEMENTS `wouldCreateCycle` item superseded: the coarse check no longer decides connections.

## Open questions
None.
