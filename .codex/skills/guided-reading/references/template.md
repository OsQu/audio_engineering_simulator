# Guided-reading document template

The skeleton, abstracted from `~/Downloads/story1.4.1review.md`. Match its tone: a calm textbook chapter,
second person sparingly, prose that explains rather than lists. Fill the `‹…›` slots. Drop a section only
when the construction rules below say it may be empty.

---

```md
# Story ‹X.Y.Z› — A Guided Reading

*‹One-line theme of the task — the physics headline, italicised.›*

A walk-through of the code added for Task ‹X.Y.Z›, written to be read top to bottom like a
chapter. Each part shows the code, says **what it does**, ties it to the **physics** where
that matters, and stops at a **🦀 Rust concept** box for anything new. Two appendices map the
concepts the project has introduced so far and what's coming.

Status: ‹implemented›, ‹gate state — "full gate green (fmt / lint / test / wasm / docs)" only if
actually run, else "gate not re-run for this reading"›, **‹committed | uncommitted›** — ‹one clause
of context›.

---

## Part 0 — What we set out to build

‹Frame the task from IMPLEMENTATION_PLAN.md: where it sits in its story, and the physical phenomenon
it must make *emerge*. End with a blockquote stating the headline requirement in plain language.›

> ‹The phenomenon, in the project's own voice — what must emerge from the voltage math, and why the
> test (not the ear) is the oracle here.›

---

## Part 1 — ‹The physics in one equation | the first thing built›

‹If the task rests on one relationship, state it first and derive the working form. Use a fenced block
for the equation. This becomes the spine the rest of the chapter refers back to.›

---

## Part ‹N› — ‹Name of the thing built (a type, a trait hook, a builder, the hot path, …)›

```rust
‹real excerpt from the diff — elide with // ... freely›
```

**What it does.** ‹Plain explanation.›

‹Physics tie-in when the code embodies a physical fact — e.g. "noise added before the gain because a real
amp's noise is generated at its input → the first stage sets your SNR".›

> ### 🦀 Rust concept: ‹name›
> ‹2–5 sentences. What the feature is, how it reads in *this* code, why it's the right tool here. Only
> include if the concept is NEW in this task (see SKILL.md step 3).›

‹Repeat 🦀 boxes for each new concept in this excerpt. Repeat Part N for each meaningful unit of the diff,
in the order a reader would walk the code.›

**Physics check (a real test):**

```rust
// ‹the actual hand-calc comment from the test›
‹the actual assertion›
```

‹Explain why the tolerance/number is what it is — tie it to the math, not hand-waving.›

---

## Recap — the through-line

‹Tie it together: the single physical input(s) the task stored, and everything that fell out of the math
with nothing special-cased — the project's "derive everything from the physics" thesis applied here.
Then one paragraph: the Rust that carried it (name the new concepts).›

---

# Appendix A — Concepts introduced so far (Stories 1.1 → ‹X.Y.Z›)

A reconstruction of the running concept index, by topic. Cumulative up to and including this task; derive
from docs/osku_rust_concepts.md and docs/osku_physics_concepts.md (capped at this task) plus prior readings.

## 🦀 Rust

**Type design** — ‹…›
**Traits & dispatch** — ‹…›
**Ownership & borrowing** — ‹…›
**Control flow & data** — ‹…›
**Project / systems** — ‹…›

## 🔊 Physics / audio engineering

**‹topic clusters mirroring osku_physics_concepts.md›** — ‹…›

---

# Appendix B — Coming attractions (what each remaining story likely introduces)

Informed guesses from `IMPLEMENTATION_PLAN.md` / `PROJECT_PLAN.md`.

**‹next task/story — title.›** *Physics:* ‹…›. *Rust:* ‹…›.

‹One entry per remaining story. End on the pattern to watch: each story pairs a physics idea with the
Rust feature that fits its shape.›
```

---

## Construction rules (load-bearing)

1. **Parts follow the code's reading order**, not the diff's file order. Lead with whatever a person
   should understand first (often a governing equation or the new type), then build outward to the hot
   path and the tests.
2. **Every code block is real.** Excerpt from the actual diff; elision (`// ...`) is fine, invention is
   not. The same goes for test assertions and their hand-calc comments.
3. **🦀 boxes are for new concepts only.** If it appeared in an earlier task, mention it in prose without a
   box. A chapter with zero new Rust concepts is legal — say so rather than padding.
4. **Physics tie-ins come from the plan's design notes**, translated into narrative — don't paraphrase the
   plan verbatim and don't improvise physics the task doesn't model.
5. **Appendix A is cumulative and capped at this task**; never list concepts from later tasks as already
   introduced.
6. **Appendix B entries are explicitly informed guesses** and pair physics ↔ Rust for each *remaining*
   story.
7. **The status line is honest** — see SKILL.md step 2. Never claim a gate you didn't run.
8. **No review content.** This explains; it never judges or suggests changes.
