---
name: guided-reading
description: Generate a "textbook version" guided-reading walkthrough of the code added for one task/story in this repo — a top-to-bottom chapter that shows each code excerpt, says what it does, ties it to the physics, and stops at a 🦀 Rust-concept box for every concept new in that task, plus a cumulative concept index (Appendix A) and informed coming-attractions (Appendix B). Use when the user asks to "write a guided reading", "do the textbook version", "walk through task X.Y.Z", "explain the changes in task/story N", or references a review/reading copy of a task's diff. Takes a task or story id as the argument (e.g. `1.4.1`, `1.4`, `1.5.2`); defaults to the most recent task if omitted.
---

# Guided Reading

Produces a chapter-style walkthrough of the code added for one task or story. It is a **learning artifact for
user**, not a code review and not PR notes: it reads top-to-bottom like a textbook chapter, pairs
each piece of code with its physics, and surfaces every Rust concept that is *new in this task*.

The deep value is the pairing the project is built on — **each task couples a physics idea with the
Rust feature that fits its shape**. The walkthrough makes that pairing explicit and cumulative.

## Argument

A task or story id: `1.4.1`, `1.4`, `1.5.2`, etc. (with or without a leading "Story"/"Task").
- A **task** id (three numbers, `1.4.1`) → walk just that task's changes.
- A **story** id (two numbers, `1.4`) → walk the whole story (all its landed tasks).
- If omitted, target the most recently completed task (newest task-naming commit, or uncommitted work).

## Inputs — read these first

| Source | Used for |
| --- | --- |
| `docs/IMPLEMENTATION_PLAN.md` | The task block (Goal / Watch out / Validate / design notes) → Part 0, and the list of **remaining** stories → Appendix B. Find the `### Story X.Y` / `**Task X.Y.Z**` entry. |
| `docs/PROJECT_PLAN.md` | Vision/roadmap context for Appendix B coming-attractions. |
| `docs/osku_rust_concepts.md` | The running **Rust** concept index. The oracle for new-vs-known and the basis for Appendix A. |
| `docs/osku_physics_concepts.md` | The running **physics/DSP** concept index. Same role for physics. |
| The task's **diff** | The actual code to walk. Resolve as below. |

## Workflow

### 1. Resolve the task and its diff (auto-detect)

Establish what code the task added:

- **Uncommitted work present** (`git status --porcelain` non-empty) and it plausibly *is* this task's work
  → walk the working tree: `git diff HEAD` plus untracked files. This is the "review copy" case (1.4.1 was
  written this way, uncommitted).
- **Otherwise** → resolve to commits whose messages name the task/story. Try
  `git log --oneline --grep="Story X.Y" --grep="X.Y.Z" -E` and inspect the messages; a task usually spans
  one primary commit plus small follow-ups (e.g. 1.4.1 was `e06d73b` + `f8fc436` + `11f6422`). Diff the
  union: `git diff <parent-of-first>..<last>`.

State the resolution to the user in one line before writing (which commits / working tree, and the file
list), so a wrong guess is caught early.

### 2. Determine the honest status line

The doc opens with a status line like:
`Status: implemented, full gate green (fmt / lint / test / wasm / docs`), **committed/uncommitted** — …`

- Read commit state from git (committed vs uncommitted) — state it truthfully.
- For the gate: if you can cheaply confirm it (the work is in the tree and you run the pre-push gate
  `source "$HOME/.cargo/env" && cargo fmt --check && cargo lint && cargo test && cargo wasm && cargo docs`),
  report the real result. If you don't re-run it, say "gate not re-run for this reading" rather than
  claiming green. Never assert green you didn't observe.

### 3. Classify concepts: new-in-this-task vs already-known

This is the core judgment and what makes the boxes worth reading.

- A concept earns a **🦀 Rust concept** box (or a physics callout) **only if it is genuinely introduced by
  this task** — i.e. it does not appear in the code or walkthroughs of any *earlier* task.
- Decide "earlier" by: (a) the concept reference docs as they stood before this task, (b) prior
  `story*review.md` walkthroughs, and (c) earlier git history. When unsure whether a concept is new, grep
  the codebase before this task's diff — if it appears earlier, it is **not** new here; mention it in prose
  without a box.
- Don't box trivia. Box a concept when it's the first time the user meets it and it carries weight (a
  language feature, an idiom, a discipline). Re-used concepts get at most a passing prose mention.
- Note (don't auto-write): if the walkthrough introduces a concept that is **missing** from
  `docs/osku_rust_concepts.md` / `osku_physics_concepts.md`, flag it to the user at the end so he can add it.
  Those docs are tracked and maintained by him — do not edit them as part of this skill.

### 4. Write the document

Follow `references/template.md` exactly — section order, the "show code → what it does → physics →
🦀 box" rhythm, both appendices. Construction rules at the bottom of that file are load-bearing.

Key content rules:
- **Real code only.** Every code block is an actual excerpt from the task's diff (lightly elided with
  `// ...` is fine). Never invent code.
- **Real tests as the oracle.** Analog-domain claims are backed by a real test with its hand-calc comment
  (this repo's §9 philosophy). Quote the actual assertion and the actual hand calc.
- **Physics from the plan.** Part 0's motivation and the physics tie-ins should track the task's design
  notes in `IMPLEMENTATION_PLAN.md`, not be improvised.
- **Appendix A is cumulative**, organized by topic (🦀 Rust / 🔊 Physics), covering everything introduced
  *up through and including* this task — reconstructed from the two concept reference docs (capped at this
  task) plus prior walkthroughs.
- **Appendix B** gives informed guesses for each *remaining* story from `IMPLEMENTATION_PLAN.md` /
  `PROJECT_PLAN.md`, each pairing a physics idea with the Rust feature that fits it. Mark them clearly as
  informed guesses.
- **Close on the through-line**: the physics thesis ("derive everything from the physics") and the Rust
  that carried this task.

### 5. Output

Write to `~/Downloads/story<id>review.md` (matching the reference's naming, e.g. `story1.4.2review.md`),
unless the user names another path. Print the absolute path. Don't dump the whole document into chat —
summarize what it covers (parts, how many new Rust/physics concepts, any missing-from-reference flags)
and let him open the file.

## Anti-patterns

- **Don't review.** No "consider refactoring", no nits, no praise. It explains, it doesn't judge.
- **Don't box known concepts.** A box on something introduced two tasks ago defeats the purpose; the
  signal is *new* concepts.
- **Don't fabricate physics or tests.** If a claim isn't backed by a real test in the diff, don't dress it
  up as one. Tie numbers to the actual hand calcs.
- **Don't edit the tracked concept docs or the plans** as a side effect — read them; flag gaps for the user.
- **Don't claim a green gate you didn't run.**
- **Don't paraphrase the plan's prose back verbatim** — translate the design notes into the reading's own
  top-to-bottom narrative.
