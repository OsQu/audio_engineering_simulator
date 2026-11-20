Remove the unused device-level wouldCreateCycle check

## What and why

The open item "make the web `wouldCreateCycle` check domain-aware" (Story 5.9 open decision (ii),
`docs/IMPROVEMENTS.md`) was already covered by the Luma hardware-insert fix
(`docs/bugs/2026-09-09-luma_loop.md`). Patch edits are now engine-validated transactions, so
`evaluateConnection` never consults a cycle check. A one-way digital loop goes to the compiler,
which breaks it at a digital/acoustic edge and rejects an all-analogue one.

`wouldCreateCycle` survived only as an exported function exercised by its own tests. This PR:

- deletes it from `web/src/connections.ts` along with its three unit tests;
- keeps the test that pins the real behaviour: a loop-closing drag is accepted for engine
  validation (now under `describe("engine-authoritative feedback validation")`);
- fixes two comments that still referred to the removed check (the duplex branch in
  `connections.ts`, a duplex test);
- marks open decision (ii) in `docs/EPIC_5_NOTES.md` resolved, pointing at the bug doc.

No behaviour change.

## What to look at

- `web/test/connections.test.ts`: the remaining loop-closing test still covers what the UI should do.
- Web gates (format, Biome, svelte-check, Vitest) and `check-plan-refs.sh` pass. No Rust changed.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
