# AES-67 — Story 10.4 Explicit faces & face-local socket geometry

## Intent
Stable face instances for studio/workbench/focus and revisioned face-local socket geometry replacing
world-space jack sweeps. Authoritative design: `docs/epics/epic-10/story-10.4-explicit-faces-socket-geometry.md`
and the Story 10.4 block in `docs/IMPLEMENTATION_PLAN.md`.

## Process constraint
Repo CLAUDE.md: each Task's program design is reviewed by Oskari before implementation. Design is drafted
in the companion file, then `ask_human` (blocking) for sign-off.

## Slices
| Task | Status |
| --- | --- |
| 10.4.1 pure face-presentation model | done (WIP commit 04efdea) |
| 10.4.2 face lifecycle integration | in progress |
| 10.4.3 revisioned socket-geometry model | planned |
| 10.4.4 face-local DOM registration & projection | planned |
| 10.4.5 browser matrix & legacy cleanup | planned |

## Decisions
- 2026-10-30 Oskari approved the 10.4.2 design and authorised proceeding through 10.4.3–10.4.5 without
  per-task review (I-131). Designs are still written into the companion file before each task.
- Flip completion via WAAPI `CSSTransition.finished` per generation — immune to reversal event ordering.
- Face slots are `display: contents` wrappers with data attributes — no prop threading through 9 faceplates.

## Open questions
- none
