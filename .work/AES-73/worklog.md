# AES-73 worklog

## 2026-11-20

**Did**
- Surveyed spaces: web-only `scene.ui.spaces`. The one room was the hard-coded `CONTROL_ROOM` constant,
  and the "+ space" button (`addSpace`) created more at runtime.
- Wrote plan.md and asked Oskari (I-439). Answer: each scene keeps its own spaces, seeded from the data
  file (no schema bump); add a Live Room; keep the per-item space selector visible.
- Implemented in one commit: `web/src/studio-spaces.json` (Control Room 4000×3000×1400, Live Room
  6000×4500×1400 mm), plus `parseSpaces` validator and `studioSpaceList()` in `scene-store.ts`.
  `defaultScene()` seeds from it. Removed `newSpace`, `DEFAULT_ROOM`, `CONTROL_ROOM`, `addSpace`, the
  "+ space" button and its CSS. Added `resolveJsonModule` to web/tsconfig. Struck the IMPROVEMENTS.md item.
- Tests: validator cases, default scene equals the data file, per-scene copies. Gates: format, Biome,
  svelte-check, Vitest (435), Playwright (40/40 on $VITE_PORT), check-plan-refs. No Rust touched.
- Live check (Playwright MCP): tabs show "Control Room | Live Room", no "+ space", selectors list both
  rooms, and the Live Room renders empty.
- `fx quality check`: ok (Δcog +13 comes mostly from the new validator, within limits).

**Learned**
- The space tabs only render after ▶ start (`session.ready`).
- Live Room dimensions were my choice (assumption). They're easy to change in the JSON.

**Next** — none; ready for review.
