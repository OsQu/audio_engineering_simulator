# Equipment Story

Store one durable execution record at `docs/equipment-stories/<type-id>.md`. It should be sufficient for
a fresh session to resume safely without chat history. Keep it concise, factual, and current; remove
superseded sketches rather than accumulating a transcript.

```markdown
# <Device name> equipment Story

- Status: planning | awaiting approval | approved | building | awaiting visual approval | blocked | done
- Brief: [`../equipment-briefs/<type-id>.md`](../equipment-briefs/<type-id>.md)
- Approved scope/date:
- Autonomous execution and per-task commits approved: yes/no
- Execution model: primary harness supervising Terra-tier implementation agents
- Branch/base, if applicable:
- Current task and exact resume point:

## Goal and boundaries

- Story outcome:
- Explicitly deferred:
- Architecture seams that must remain unchanged:
- Stop conditions:

## Task plan

### Task 1 — <one coherent outcome> · planned | building | blocked | done

- Depends on:
- Owned files:
- Shared seams/aggregation edits:
- Terra implementer assignment/status:
- Program design: <only useful call tree, file-tree delta, or key signatures>
- Acceptance checks:
- Validation commands:
- Commit subject:
- Implementer report: <changed files, checks run, uncertainty or gaps>
- Primary verification: <diff review, independently rerun checks, workbench evidence if relevant>
- Delivered/decisions/deviations: <fill while executing>
- Validation result: <fill before done>
- Resume point: <keep exact while building; clear or point to next task when done>

### Task 2 — ...

## Visual approval gate

- Workbench URL and fixed view:
- Evidence/screenshots:
- Manager grade history: <iteration, overall 0–5 score, category evidence, required corrections>
- Current manager score:
- Identity-critical categories all at least 4: yes/no
- User decision/date:
- Requested changes:
- Sonic tasks released by approval:

## End-to-end DSP approval gate

- Unit-test coverage and numeric oracles:
- Behaviors only covered at integration level, with reason:
- Representative workbench patch/configuration:
- Exact user actions:
- Expected audible results:
- Expected meter/readout/control results:
- Manager preflight evidence and unavailable capabilities:
- User decision/date:
- Requested corrections and retest result:

## Architecture gap

- None, or link/summarize the gap report in the equipment brief and state the exact resume point.

## Story completion

- Final validation:
- Task commits: <ordered subjects; hashes are recoverable from git log>
- Delivered:
- Intentionally shallow:
- Deferred follow-ups:
```

## Maintenance rules

- Write the Story after research-brief approval and before implementation begins.
- Set `awaiting approval` when the task plan is ready. Set `approved` only after the user explicitly
  approves both the plan and autonomous per-task commits.
- Make each task exactly one focused commit. If it is too large or mixes outcomes, split it before work.
- A task may depend on prior tasks, but it must describe everything a fresh session needs to execute it.
- Use a Terra-tier subagent for implementation and keep commit authority with the primary supervising
  session. Do not treat the implementer's report as independent verification.
- Keep the visual task `building` while the manager score is below 4 and while user approval is pending.
  A score of 4–5 opens the user review gate; only recorded user approval releases DSP work and allows the
  visual task to be marked done and committed.
- Cover DSP with focused deterministic unit tests as far as existing seams permit. Keep the final DSP/
  end-to-end task `building` until the manager has preflighted a representative workbench path and the
  user has jointly verified its audible and visible behavior.
- Update status and the exact resume point before yielding at any gate or blocker.
- Include the Story update in the same commit as the task it records. Record the intended commit subject,
  not its self-referential hash.
- Never mark a task done on failed or unrun required validation. State unavailable visual/browser checks
  plainly rather than treating them as passed.
- Do not edit an approved goal silently. Record a small implementation-level deviation; return to the
  user for a material product/scope decision.
