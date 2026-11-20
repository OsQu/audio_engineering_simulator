# AES-73 — Describe spaces in the scene file instead of hard-coding them

Source: `docs/IMPROVEMENTS.md` — "Do not allow creating new spaces, but describe them in the scene/space
file. So from user perspective the layout is hard coded."

## Intent

Spaces (rooms) become **authored data**, not something the user makes at runtime. The studio's rooms are
declared in a readable data file; the UI shows them and lets gear move between them, but there's no
"+ space" control. The scene schema stays readable and old saves keep loading.

## Where things stand (survey)

- Spaces are web-only: `SceneUi.spaces: Space[]` (`{ id, name, room: { width, depth, height } }`) in
  `web/src/scene-store.ts`. The engine never sees them. The scene is saved as versioned JSON
  (`SCHEMA_VERSION = 18`; a version mismatch is discarded, no migration).
- The only room definition is the TS constant `CONTROL_ROOM = newSpace("control-room", "Control Room")`
  using `DEFAULT_ROOM` (4000 × 3000 × 1400 mm), baked into `defaultScene()`.
- Runtime creation: `addSpace()` in `scene-ops.ts`, wired to the "+ space" toolbar button in
  `App.svelte`. Tested in `web/test/scene-ops.test.ts`.
- Moving gear between existing spaces: `moveDeviceToSpace` / `moveRackToSpace` + the per-item
  `<select class="space-select">`. These don't create spaces, so they stay.
- Placements that reference a missing space already resolve to `unresolved` (`effective-placement.ts`),
  so a bad space id doesn't crash anything.

## Approach (proposed)

1. **A spaces data file.** Add `web/src/studio-spaces.json`: a plain JSON array of
   `{ id, name, room: { width, depth, height } }` (world mm), which is the exact `Space` shape already in
   the save file. `defaultScene()` takes its `ui.spaces` from that file. `CONTROL_ROOM`, `DEFAULT_ROOM`
   and `newSpace` go away. The default placements keep using the first space's id (`control-room`).
   A small validator (`parseSpaces`) checks the shape at module load and fails loudly in dev/tests if the
   file is malformed (unique ids, at least one space, positive dimensions).
2. **No runtime space creation.** Remove the "+ space" button, `addSpace` and its test. Keep the move
   selectors as they are (I-439).
   The space tabs stay as the room switcher.
3. **Schema: unchanged, no version bump.** `ui.spaces` keeps its shape and its place in the save, so
   every v18 save (localStorage, bench URLs) loads as before. The scene file still describes its own
   rooms, readable in the JSON. Nothing in the format implies "user-created".
4. **Tests.** Default-scene test asserts spaces come from the data file. Validator unit tests cover a
   good file, duplicate ids, empty list and non-positive dimensions. Remove the `addSpace` test. Run
   the browser suite to check nothing referenced the "+ space" button.
5. **Docs.** Strike the item in `docs/IMPROVEMENTS.md`, and add one line on where rooms are authored.

## Slices

| # | Slice | Status |
|---|-------|--------|
| 1 | Spaces data file + validator; `defaultScene` reads it; drop `CONTROL_ROOM`/`newSpace` | done |
| 2 | Remove `addSpace` + "+ space" button (selectors stay) | done |
| 3 | Tests, IMPROVEMENTS.md, gates | done |

## Decisions (approved, I-439)

- (a) **The scene keeps its spaces, which are seeded from the data file.** No schema bump.
- (b) **Add a Live Room** alongside the Control Room in the data file (empty, since no default gear is
  placed there).
- (c) **Keep the per-item space selector visible** even with one space. (With two rooms it always has
  two options anyway.)

## Decisions (original proposal, for reference)

- **The scene keeps its spaces, which are seeded from the data file.** Each saved scene carries its own
  `ui.spaces`, as today. *Alternative:* the data file is authoritative and on load it overrides a save's
  spaces, so editing the file reshapes every existing save. That's a stronger "hard-coded layout", but
  a save can disagree with the file (placements pointing at a removed room become unresolved), and it
  changes what "load" means. I recommend the first option. It's backward compatible and the smaller change.
- **No schema bump.** The shape is unchanged; a bump would throw away everyone's current save for no gain.
- **Rooms stay a single Control Room** unless Oskari wants more authored rooms (e.g. a Live Room) in the
  same change.
- **Only the space list moves to JSON, not the whole default scene.** Turning all of `defaultScene()` into
  JSON would lose its explanatory comments and is a bigger change. Can follow up if wanted.

## Open questions (answered in I-439, see Decisions)

1. Scene carries its spaces (recommended) vs data file is authoritative over saves?
2. Add any rooms beyond "Control Room" now?
3. OK to hide the per-item space selector when only one space exists?
