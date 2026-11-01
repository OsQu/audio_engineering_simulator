# AES-68 — Story 10.5: Resolved presentation, stable layers & rack assemblies

## Intent
Studio and workbench paint objects in a deterministic, resolver-owned physical order (named layers,
silhouettes, rack parentage); hover/focus/grace/drag can no longer change occlusion; toolbars live in an
object-ID-keyed interaction overlay; eject/remove-rack follow U3 atomically.

## Approach
Plan with the story-planning skill → companion design `docs/epics/epic-10/story-10.5-resolved-presentation-stable-layers.md`
→ **Oskari reviews design (CLAUDE.md §2) before any implementation** → tasks one by one, gates green, no
product commits (only `.work/` + docs per factory flow, after review).

## Slices
| Task | What | Status |
| --- | --- | --- |
| 10.5.1 | Pure ResolvedObject: layers, total order, parentage, silhouettes | done |
| 10.5.2 | Studio: resolved order + z bands + interaction overlay; graduate hover regressions | done |
| 10.5.3 | Workbench: same | planned |
| 10.5.4 | U3 atomic ejection planner | planned (design drafted) |

## Decisions (approved by Oskari, I-151, 2026-11-01)
- Layer follows the *settled* face side — no reorder mid-flip; switches with lead reveal (U1).
- Racks/occupants stay DOM siblings — parentage is data; 10.3 preview already moves the assembly.
- Silhouette rect in world mm (same as ResolvedObject.rect); 10.6 projects for masks. Bench rect null.
- U3 search on GRID_MM lattice only (doc says "existing free grid"), in the rack's wall regardless of view.
- Eject/remove-rack chips disabled while their owner flips (avoid transitionOwner changing mid-flip).

## Resolved questions (I-151: "approved 1")
- Interaction overlay tab order: accepted (toolbars leave the chassis subtree, tab after the objects).
- U3 failure: reported through the existing status line.
- I-152: 10.5.2 design approved. Oskari: "work the whole story; commit each task" (overrides the
  per-task stop/no-commit gate for this ticket).
