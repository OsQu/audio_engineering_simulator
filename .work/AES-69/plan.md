# AES-69 — Story 10.6: Unified cable, portal & held-lead presentation

## Intent
Ordinary cables, portal stubs/chips and held leads all derive from one pure endpoint resolution: shared
endpoint identity, nearest-edge hidden-face exits, preview following, U1 suppression, and resolver-owned
main/foreground mask sets that also clip hit paths.

## Approach
story-planning skill → companion design `docs/epics/epic-10/story-10.6-unified-cable-portal-held-lead.md`
+ thin Story block in IMPLEMENTATION_PLAN.md → **Oskari reviews the design (CLAUDE.md §2) before any
implementation** → tasks one by one, gates green.

## Slices
| Task | What | Status |
| --- | --- | --- |
| 10.6.1 | Pure `cable-resolution.ts`: endpoint states, exits, paths, mask passes, portal chips | planned |
| 10.6.2 | Studio cables + portals render from resolved connections; clips on paint + hit; preview following | planned |
| 10.6.3 | Workbench exits/masks (U2) + listening badge | planned |
| 10.6.4 | Held leads on the shared pipeline; legacy removal; browser matrix | planned |

## Decisions (proposed — awaiting review)
- Surface coordinates (camera-independent, y-down) for all cable resolution — where anchors already live.
- Exit edge from the hidden socket's see-through projection, tie order bottom/right/left/top; EXIT_LEAD 24.
- Nested evenodd `clip-path` per excluded silhouette (not `mask`) — overlaps intersect correctly and
  clip-path also clips hit testing.
- Portal chip base = socket point or exit lead point; old offsets saved against the estimate shift (accepted).
- Portal chip → interaction overlay; stub → main/foreground passes like any cable.

## Open questions
- Held-lead occlusion: (A, recommended) "in hand" — paints above objects, masked only by a hidden-face
  source owner; or (B) fully physical like a settled cable (disappears behind front panels incl. the
  destination jack's panel).
- Base: stacked on AES-68 (10.5 not yet merged to main) — as the ticket specifies.
