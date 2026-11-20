Describe the studio's spaces in a data file instead of creating them at runtime

## What and why

The studio's rooms used to be a hard-coded TS constant (`CONTROL_ROOM`), and the operator could add more
with a "+ space" button. Now the layout is authored data and stays fixed from the user's side:

- **`web/src/studio-spaces.json`** declares the rooms in the same `{ id, name, room: { width, depth,
  height } }` shape the save file already uses: a **Control Room** (4 × 3 m, as before) and a new, empty
  **Live Room** (6 × 4.5 m).
- **`parseSpaces`** (`scene-store.ts`) validates the file at module load: non-empty, unique ids, names,
  positive dimensions. A malformed file fails loudly instead of producing a broken room.
- **`defaultScene()`** seeds `ui.spaces` with a fresh copy (`studioSpaceList()`). Default gear still
  stands in the control room.
- **Removed:** `addSpace`, `newSpace`, `DEFAULT_ROOM`, the "+ space" toolbar button and its CSS.
  Gear and racks can still move between the authored rooms through the existing space selectors.

## Compatibility

The scene schema is unchanged (still v18). Each scene still carries its own `ui.spaces`, so existing
localStorage saves and bench URLs load as before. An old save made before this change has only the
Control Room. It won't gain the Live Room until you reset to the default scene, which is deliberate:
the saved scene is the source of truth for its own rooms.

## Look closely at

- `web/src/scene-store.ts`: `parseSpaces` / `studioSpaceList` and the copy semantics. Scenes must never
  share room objects with the authored list.
- `web/tsconfig.json`: adds `resolveJsonModule` for the JSON import.
- The Live Room size is a placeholder choice. Adjust it in the JSON if you like.

## Verified

`pnpm run format`/`check`/`typecheck`/`test` (435), the Playwright suite (40/40), and a live check:
the space tabs read "Control Room | Live Room" with no "+ space", and the Live Room renders empty.
No Rust changes, so no WASM rebuild is needed.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
