---
name: story-planning
description: Flesh out the next Story in the current Epic from a coarse sketch into an agreed, task-level plan written into docs/IMPLEMENTATION_PLAN.md. Reads both plans + the current Epic's settled decisions + the previous Story's delivery, cross-checks the sketch against the actual landed code, surfaces gaps, and asks clarifying questions until the plan is solid — then writes the elaborated Story block, creates the Story branch, and seeds the harness task list. Use when the user says "plan the next story", "plan story X.Y", "let's flesh out story X.Y", "story planning", or picks up a new Story to build. Takes an optional Story id (e.g. `3.4`); defaults to the next not-done Story in the current Epic.
---

# Story Planning

Turns a Story from its deliberately-coarse sketch into an **agreed, task-level plan**, written into
`docs/IMPLEMENTATION_PLAN.md` in the project's own house style. This is the **short-horizon** step of the
three planning horizons (see below): the moment a Story is *elaborated to Task level and design notes*,
which per the plan's detail-gradient convention is exactly when it gets picked up to build.

**Scope — planning only.** This skill plans the next Story and hands off to execution. It does **not**
write the `*Delivered:*` block (that is recorded after the Story is built) and does **not** do the
post-Epic migration into `EPIC_N_NOTES.md` + Epic summary. Those are separate, later concerns — treat
them as background context, not work this skill performs.

**It is a dialogue, not a one-shot.** The value is in the assessment: cross-check the coarse sketch
against reality, surface gaps and feasibility risks, and **ask clarifying questions until the plan is
solid.** Only then write it down. Do not jump straight to a task list.

## The three planning horizons

The plan is a living document, elaborated near and coarse far (`IMPLEMENTATION_PLAN.md` "How this plan is
structured"). Hold all three in view when planning:

- **Long (Epic):** the roadmap stage — enough to see the arc we're trying to achieve. Completed Epics are
  *summarised* in the plan; their full detail lives in `docs/EPIC_<N>_NOTES.md`.
- **Mid (current Epic):** all its Stories are named and sketched, with the Epic's **settled architecture
  decisions** that constrain every Story.
- **Short (this Story):** planned all the way to Task level — what this skill produces.

## Argument

An optional Story id: `3.4`, `4.1`, etc. (with or without a leading "Story").
- Given → plan that Story.
- Omitted → the **next not-done Story in the current Epic** (the first without `✅`/`Done`, in order).
State which Story you resolved in one line before doing anything else, so a wrong guess is caught early.

## Inputs — read these first

| Source | Used for |
| --- | --- |
| `docs/IMPLEMENTATION_PLAN.md` | The target Story's coarse sketch (Goal / watch-outs / any settled bullets), the **Epic's settled architecture decisions** (the bullets under the Epic header that constrain every Story), and the **previous Story's `*Delivered:*`** block (what actually shipped, deviations, known simplifications). |
| `docs/PROJECT_PLAN.md` | The *what and why* — vision, §5 engine design, §9 roadmap, §10 risks/open questions. Anchor the Story's goal and watch-outs to these, don't improvise them. |
| `docs/EPIC_<N>_NOTES.md` | When the relevant decisions/oracles live in a **completed** Epic's notes (the plan only keeps the constraining summary). |
| The **actual code** (`crates/`, `web/`) | Cross-check the sketch against what's really landed — the "Docs + code surface" rigor. The plan documents an engine public surface; **spot-check the specific API this Story leans on actually exists** (names, signatures), rather than trusting the doc. |
| `CLAUDE.md` | The non-negotiable architecture decisions (§5), Rust conventions (§6), rate model (§7), workflow (§8). A task that violates one is a bug, not a style choice. |

## Workflow

### 1. Resolve the target Story and its base

- Resolve the Story id (arg or next not-done). State it in one line.
- Check git state: the workflow is **one branch per Story off `main`** (`CLAUDE.md` §8). Confirm the base —
  normally the previous Story is merged and we branch from an up-to-date `main`. If the previous Story isn't
  merged yet, or `git status` is dirty, surface that and ask how to proceed rather than assuming.

### 2. Read the inputs and reconstruct context

Read the target Story's sketch, the **Epic's settled decisions**, and the **previous Story's `*Delivered:*`**
block. The previous delivery is the most important context: it records deviations from plan, known
simplifications, and the real API that landed — all of which reshape the next Story.

### 3. Cross-check the sketch against reality (the gap pass)

This is the heart of the skill. The Story sketch was written coarsely and earlier work routinely changes
its shape — so **verify before planning tasks:**

- **Sketch vs. landed code:** does the API this Story assumes actually exist as named? (e.g. a setter, a
  param handle, a node, a queue.) Spot-check the real `crates/`/`web/` code, not just the plan's
  surface summary. Flag every drift.
- **Sketch vs. previous delivery:** did the last Story defer, rename, or simplify something this Story
  depends on? Did it leave a "known simplification" this Story must now address (or inherit)?
- **Feasibility & constraints:** does anything in the sketch collide with a `CLAUDE.md` non-negotiable
  (zero-alloc/panic-free/lock-free hot path, the signal-type split, units-as-newtypes, determinism, the
  rate model)? Is anything under- or over-scoped for a Story (≈a week; Tasks are 1–10 commits each)?
- **Open questions:** collect the genuine unknowns — including any `*Open question (resolve at story
  pickup)*` already noted in the sketch.

### 4. Surface gaps and ask — iterate until solid

Present the gaps, risks, and open questions concisely, then **ask clarifying questions** (use
`AskUserQuestion` for crisp forks). Iterate with the user — do not proceed to a final task breakdown until
the plan is genuinely solid and the open questions are resolved or explicitly deferred. Resolving an open
question by deferring it is fine; record *that* as the decision.

### 5. Break into tasks

Once the design is agreed, decompose into **Tasks doable one-by-one**, each **1–10 commits** (the plan's
unit of execution), in dependency order. Each Task should be a coherent slice with a clear done-state.
Where the analog domain is involved, the Task's validation is a **hand-calc oracle** (a number computed by
hand, asserted in a test with the calc in a comment — `CLAUDE.md` §9), not ears.

### 6. Write the elaborated Story block into the plan (in place)

After agreement, edit `docs/IMPLEMENTATION_PLAN.md` in place, replacing the coarse sketch with the full
Story block. **Match the existing section rhythm exactly** (see a completed Story like 3.3 for the
template):

- `### Story X.Y — <Title> — 🚧 **In progress**` — mark the Story in-progress with this suffix (the status
  convention: future Stories are unmarked, the active one is `— 🚧 **In progress**`, completed ones become
  `— ✅ **Done**`). Also update the Epic's running **Progress** line so it no longer advertises anything this
  planning pass deferred or changed.
- `*Goal:*` — what this Story delivers and why, anchored to `PROJECT_PLAN.md`.
- `*Watch out:*` — the traps and constraints (hot-path contracts, clock/units pitfalls, scope guards).
- `*Design notes (settled at planning):*` — the decisions reached in step 4, **with the rejected
  alternative and the why** where it matters (this is what makes the block worth re-reading). Note known
  simplifications explicitly as "not a bug".
- `- **Task X.Y.Z** — …` — the task list from step 5, each with its done-state / validation.
- `*Validate:*` — the Story's exit gate (the `✅ met` marker is added later, when the Story is done).

Do **not** write a `*Delivered:*` block — that is recorded after the work, outside this skill.

Per the task loop below: **do not commit.** The user verifies and commits the plan edit himself.

### 7. Hand off to execution

After the plan is written and agreed:

- **Create the Story branch:** `e<epic>-s<story>/<short-story-slug>` (`CLAUDE.md` §8), off the agreed base.
  (Branching is normal repo work, not a commit — fine to do.)
- **Seed the harness task list:** `TaskCreate` one entry per planned Task, so the work can be executed
  one-by-one. This is the entry into the task loop.

## The working model (the task loop) — always honor this

This governs every body of work (`CLAUDE.md` §2) and is included here so it stays in context while
planning *and* executing:

1. **Create tasks** for the work before starting (step 7 seeds them).
2. After completing a task, make sure it **compiles, lints, and passes tests** before reporting it done.
   The full pre-push gate (mirrors CI):
   `source "$HOME/.cargo/env" && cargo fmt --check && cargo lint && cargo test && cargo wasm && cargo docs`.
3. **Do not commit.** Stop and let the user verify what was done.
4. **Discuss and follow up** on any changes together — the user commits the code himself.
5. When the user says he has verified and committed, **review his commit message** to confirm it
   accurately reflects the work.

Never run `git commit` unless explicitly asked. Committing is the user's verification gate.
**System-modifying commands are the user's to run** (package installs, toolchain/global config) — surface
the exact command via `! <command>`; editing repo files and running project-local `cargo` tooling is normal
work. (Toolchain note: this shell doesn't source `~/.zshenv`, so prefix cargo with
`source "$HOME/.cargo/env" &&`.)

## Anti-patterns

- **Don't skip the gap pass.** Jumping from sketch → task list without cross-checking against the landed
  code and the previous delivery is the main failure mode — the sketch is coarse *by design* and usually
  stale in places.
- **Don't plan past clarity.** If an open question materially shapes the tasks, ask — don't guess and
  build a plan on the guess. Deferring is a valid resolution; record it as one.
- **Don't improvise the goal/physics.** Anchor Goal and watch-outs to `PROJECT_PLAN.md` and the Epic's
  settled decisions, not invention.
- **Don't violate the non-negotiables** when shaping tasks (hot-path contracts, signal-type split,
  units-as-newtypes, determinism, rate model — `CLAUDE.md` §5–7).
- **Don't write `*Delivered:*` or migrate to `EPIC_N_NOTES.md`.** Out of scope — planning only.
- **Don't commit, and don't run system-modifying installs.** Surface them; the user runs them.
- **Don't over-plan future Stories.** Elaborate only the Story being picked up; leave the rest coarse.
