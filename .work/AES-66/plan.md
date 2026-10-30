# AES-66 — Story 10.3: Effective placement hierarchy & live previews

## Intent
One pure effective-placement path (stored location, rack inheritance, U-slots, projection) plus
free/rack/mount/workbench previews owned by App and Workbench. Program design lives in
`docs/epics/epic-10/story-10.3-effective-placement-hierarchy.md`.

## Slices
- 10.3.1 pure effective-placement resolution — done (Oskari, 907be34)
- 10.3.2 pure preview resolution — done (0ff67f7)
- 10.3.3 App-owned studio previews — done (a8f7a57)
- 10.3.4 Workbench-owned previews — done (890dd5f)
- Story Validate gate + close-out — done (this session, b8268bc)

## Decisions
- Added eject-candidate and all-walls inheritance oracles: the only Validate items without an explicit test.
- Graduated the Story 10.1 known regression `rack-preview-leaves-occupant-behind` to
  `web/test-browser/placement-preview.spec.ts`: it started passing (graduation mechanism fails closed), and
  10.3's App-owned rack preview is what fixed it, although it named Story 10.5 as owner.
- Committed Prettier output for Svelte files left unformatted by earlier work, as a separate commit.

## Open questions
- None blocking. Story 10.2's heading in IMPLEMENTATION_PLAN still says "In progress" while Epic progress
  says complete — left untouched (not this ticket's scope).
