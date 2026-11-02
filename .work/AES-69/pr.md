AES-69: Story 10.6 — Unified cable, portal & held-lead presentation

Stacked on AES-68 (Story 10.5). Design: `docs/epics/epic-10/story-10.6-unified-cable-portal-held-lead.md`
(approved by Oskari, I-163 — held leads "in hand", option A).

## What changed and why

Before this story, three cable-like things worked out their endpoints three different ways:

| Kind | How it found its endpoints |
| --- | --- |
| Full cable | `cableEndpoints`, with a 38 %/62 % interior estimate for hidden sockets and a straight-segment clamp the sagging curve could re-enter |
| Portal stub | `cableAnchor` alone, then painted above everything |
| Held lead | A cached `srcPoint` captured when the jack was pressed |

Masks came from `CableLayout` callbacks shaped around CSS z-bands. Cables used committed rects, so they lagged behind drag previews. Hit paths could be clicked where the lead was invisible.

Now one pure module, **`web/src/cable-resolution.ts`**, owns all of it, in surface coordinates.

- **Endpoint states.** Every end resolves to one explicit state: `socket`, `exit`, `off-view`, `suppressed` (U1), `pending` or `unresolved`. A connection is `full`, `portal` or `none` (with a reason); nothing falls back to an estimate.
- **Hidden-face exits.** A hidden-face end leaves at the owner edge nearest its see-through socket position. Ties go bottom → right → left → top. A 24-unit straight lead runs outward before the sag. The other endpoint never influences the edge.
- **Mask passes.** The resolver emits silhouette sets per pass:
  - `main` excludes hidden-face owners;
  - `foreground` includes a front socket's owner and excludes everything ordered above it;
  - `held` excludes only a hidden-face source.

  Renderers turn the sets into nested `<g clip-path>` groups via `SilhouetteClips.svelte` and `CablePass.svelte`. Clip-path also clips hit testing, so invisible segments can't be selected.
- **Portals.** A stub takes exactly the ordinary passes. Its chip sits at the local physical end plus the persisted offset.
- **Held leads.** `PatchState.srcPoint` is gone. `resolveHeldCable` re-resolves the logical source every frame, so the lead follows previews, view switches and settled flips.
- **Wiring.** `studioCableInput` and `benchCableInput` (`view-resolver.ts`) build the input from the preview-aware resolved objects (10.3/10.5). `patch.project` uses the same frames and the settled face side.
- **Workbench (U2).** It uses the same exits and masks instead of mirrored see-through attachments. A support device whose monitored socket is hidden shows a labelled "listening · {side}" badge.
- **Deleted:** `CableLayout`, `sceneCableLayout`, `benchCableLayout`, `cableEndpoints`, `cableAnchor`, the clamp, the tip patch, `bothInView`/`oneInView`, `portalOffset`, and `leadsShown`/`connShown`.

## Look closely at

- `settledPasses` and `passVisibleAt` in `cable-resolution.ts`: the mask contract (source design §6, points 3–5).
- `CablePass.svelte` (a recursive snippet nests the clip chain) and the evenodd outside clips in `SilhouetteClips.svelte`.
- `studioSlots` in `App.svelte` and `benchSlotMap` in `BenchStage.svelte`: the presented side is now the *settled* face side, not the durable facing. They differ only mid-flip, when leads are suppressed anyway.
- Portal chips stay in the overlay SVG, above physical content, rather than the HTML interaction div (I-221).
- Foreground passes have no hit path; selection stays with the main pass.
- Persisted portal offsets now apply from the real local end, so chips saved against the old interior estimate shift slightly on load. The schema is unchanged.

## Known / deferred

- **For 10.7:** a connected rear socket's centre is covered by its own cable's main-pass hit path, so you can't press it directly. This predates the story (the old `oneCable` drew the same thing) and belongs to 10.7's exact-socket-wins target arbitration.
- **`cargo docs`** fails on the existing unresolved `[instantiate]` link in `crates/devices`. There are no crate changes here; AES-68 already proposed a fix.

## Tests

- **Vitest:** 368 passing.
  - New `cable-resolution.test.ts` (21): hand-calc exit oracles, mask matrix, portal invariance, held leads.
  - `view-resolver.test.ts`: the contract matrix now runs through the resolver, plus preview following and `benchCableInput`.
- **Playwright:** 32 passing, plus the 2 expected 10.7 known regressions.
  - New `cable-presentation.spec.ts` (7): socket meet, edge exit and clip chain, portals, drag-preview following, workbench badge, held lead across a view switch and a flip.
  - Stress run ×6 of the cable, face and layer specs: 126/126.
- **Typecheck and Biome:** clean.
- **Rust:** fmt, lint, test (534) and wasm green.
