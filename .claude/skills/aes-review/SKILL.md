---
name: aes-review
description: Read-only code review of a branch or PR in the audio engineer simulator, checking it against its ticket or Story and this repo's non-negotiables (voltage physics, the VoltageBuffer/SampleBuffer split, hot-path discipline, the catalog/web split, the WASM boundary, no plan ids in source). Use when asked to review a branch, a PR or a factory ticket's change in this repo, and as fx's review skill. Takes an optional PR number or `--base <ref>`; defaults to the current branch against main.
argument-hint: "[<pr-number> | --base <ref>]"
---

# AES review

Review a change to this repo without touching the checkout. You are a reviewer, not the author: judge
the change from the code, its brief and the repo, not from the author's reasoning.

## Rules

- **Read-only.** No edits, commits, checkouts, fetches, pushes, or posting on GitHub. When run by fx,
  findings go in the review you hand back, not on the PR.
- **Ground every finding.** Re-read the code or run it before reporting. Drop what you can't back up;
  if something can't be checked, say "verify" once at the end of that finding. Few real findings beat
  many guesses.
- **Don't relitigate settled decisions.** CLAUDE.md §5 and §7 and the plans' decisions are settled; a
  change that follows them is right even if you'd design it differently. Flag a decision only when the
  change contradicts one, or when its own new decision looks wrong.

## 1. Gather the change

- Local (default): base is `--base <ref>`, else `main`. Read `git log <base>..HEAD` (oldest first),
  `git diff <base>...HEAD`, and `git status` (uncommitted work is part of the review; say so).
- PR: `gh pr view <n>` (description, comments) and `gh pr diff <n>`. Don't check it out; the local
  tree is context only.
- Nothing ahead and a clean tree: say there is nothing to review and stop.

## 2. Read the brief

What the change was supposed to do, in this order, whichever exist:

1. The ticket (fx puts it in your prompt) and its instructions.
2. The Story/Task in `docs/IMPLEMENTATION_PLAN.md` and its companion in `docs/epics/epic-<N>/`: honor
   its **Goal / Watch out / Validate** notes. They encode traps the code doesn't show.
3. The commit messages and the PR description or `.work/<KEY>/pr.md`.

Read `.work/<KEY>/plan.md` and `worklog.md` **last**, as the author's claims to check against the code,
not context to adopt: flag decisions that look wrong and claims ("tested in the browser", "gate green")
the code or tree doesn't back up.

## 3. Read the code around it

For each non-trivial changed area: read the whole changed file, grep callers of anything renamed,
removed or changed in meaning, and find its tests. For a change crossing crates, follow it across:
`engine` → `devices` (catalog, Patch IR, `build_patch`) → `wasm-bindings` (`SceneEngine`, `catalog`,
`parse_patch`) → `web/` (`engine.ts`, `session.svelte.ts`, `web/worklet/`).

## 4. Check, in priority order

1. **Correctness.** Bugs, edge cases, wrong assumptions about callees. Give the concrete input or
   sequence that goes wrong. In the analog domain, redo the hand calc: units, dBu/dBV/dBFS references,
   impedance dividers, Thévenin sources, cable R·C, sample-rate and oversampling arithmetic.
2. **Physics is emergent.** Levels, loading loss, clipping, noise, DC, phantom power and hum must come
   out of the voltage model. Flag a flag, special case or lookup that fakes an effect the physics
   should produce, and anything that derives volts from a digital or UI value instead of the reverse.
3. **The signal-type split.** `VoltageBuffer` (oversampled linear volts) and `SampleBuffer` (digital)
   only meet in an AD/DA converter. Flag any other path between them, a shared rate, a type that
   expresses a global analog↔digital rate relationship, or a buffer that stores dB.
4. **Hot-path discipline** in anything reached from `process` / `render_quantum` / the worklet: no
   allocation (`Vec` growth, `Box`, `format!`, `collect`, `to_string`, `clone` of heap data), no
   `unwrap`/`expect`/`panic!`/panicking index/`Result`, no locks, no `thread_rng`/`Instant::now`/
   `SystemTime`. Validation and allocation belong in construction and `compile`. Check denormal
   flushing in new recursive filters.
5. **Determinism.** Randomness only through the seeded `Rng`; same seed ⇒ same output. New `rand`
   dependencies keep `default-features = false`.
6. **The engine/UI contract.** A change to `catalog()`, `cable_catalog()`, `parse_patch`, the Patch IR
   or a postMessage shape needs the matching `web/` change, and saved scenes/patches keep loading (or
   the change says why not). The UI only uses the published engine API. Note that the browser won't see
   it until `pnpm run wasm`; flag a PR text that claims browser verification without that rebuild.
7. **Catalog vs web.** Catalog entries hold specs (ports, params, ranges, connector, dimensions); look
   and feel (`skin.ts`, `focus.ts`, faces) lives in `web/`. Flag presentation fields in the catalog.
8. **Web traps.** `$state` values posted to the worklet without `$state.snapshot(...)` (DataCloneError);
   comments added to `biome.json`; defensive CSS selectors papering over a stale Vite style module.
9. **Brief.** Does the change do all the ticket/Task asks, and nothing it didn't? Unasked scope is a
   finding; so is an unmet Validate note.
10. **Tests.** Analog-domain tests assert a hand-computed number with the calc in a comment; floats use
    `approx`, never `==`. New behaviour is tested, and the test would fail without the change.
11. **Conventions lint won't catch.** Units as newtypes with explicit named conversions; `f32`
    storage, `f64` for accumulators and filter state; `<name>.rs` + `<name>/` modules, no `mod.rs`.
    No plan ids (Story/Task/Epic numbers, plan section refs, `osku_*`), ticket keys or fx mentions in
    source, comments or test names; comments in the present tense.
12. **Simplification.** Code that could be smaller or plainer without losing behaviour, or that
    duplicates something that already exists.

Skip what the gate enforces (fmt, clippy, compile, failing tests, wasm build, doc links,
`check-plan-refs.sh`, Biome, svelte-check) unless you can show it missed something. When run by fx,
the `gate` and `web` checks run those separately.

## 5. Report

Start with a one-line verdict: **approve** (nothing worth fixing), **changes** (something should be
fixed before merging) or **comment** (nits only). Then the findings, most severe first, numbered:

```
### 1. Blocker — `crates/engine/src/x.rs:123` — <headline>
What is wrong and how it fails (the input or sequence), in 1–3 sentences. Fix: <suggestion>.
```

Severities: **Blocker** (wrong behaviour, broken contract, a §5 non-negotiable violated), **Should
fix** (missing test for real logic, convention violation, unmet brief), **Nit**. Direct statements, no
hedging. Over 10 findings on a large change: show the top 10 and say so. End with a short **Notes**
section only if something is actionable (e.g. "needs `pnpm run wasm` before browser testing").
