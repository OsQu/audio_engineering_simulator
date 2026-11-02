# AES-70 — Story 10.7: Unified patch targets & gesture lifecycle

## Intent
Patchable hit targets derived from resolved sockets (screen-space tolerance, exposure, blockers), and one
capture-phase ownership + pointer lifecycle for controls, sockets, cables, device moves and panning on
both stages. Closes the last two known regressions (owner 10.7).

## Approach
story-planning skill → companion design `docs/epics/epic-10/story-10.7-unified-patch-targets-gesture.md`
+ thin Story block in IMPLEMENTATION_PLAN.md → **Oskari reviews the design (CLAUDE.md §2) before any
implementation** → tasks one by one, gates green, commit per task (factory flow, AES-68/69 precedent).

## Slices
| Task | What | Status |
| --- | --- | --- |
| 10.7.1 | Pure `patch-targets.ts`: resolveSocketTargets + pickSocketTarget | planned |
| 10.7.2 | Pure classifyPress, pointer-session reducer, patching release re-resolution | planned |
| 10.7.3 | Studio arbiter + adapters; rejection reason UI; graduate known regressions | planned |
| 10.7.4 | Workbench arbiter, blockers, Story browser/manual matrix | planned |

## Decisions (approved by Oskari as-is, I-222, 2026-11-02 — see companion file "Settled decisions")
- Targets derive from `CableResolutionInput` — it already carries everything eligibility needs.
- Client-px picking: 8/12 px tolerances, exact > nearest > order > id; legality-blind picker.
- Capture-phase arbiter with direct dispatch (rejected: ownership marker read by each adapter).
- One shared pointer session; release ≠ cancel; clear ownership before releasing capture.
- Illegal drag release → pending pick with visible reason (symmetric with click-click).
- Focus windows non-patchable + blockers.
- Base: stacked on AES-69 (10.6 not merged), as the ticket specifies.

## Open questions
- none
