---
name: add-audio-equipment
description: Research, design, and add new audio equipment to this repository with a look-and-feel-first workflow using the device workbench. Use when adding a gear-inspired device or family of devices; do not use for generic engine nodes or unrelated UI widgets.
---

# Add Audio Equipment

Add recognizable, educational audio gear without making a 1:1 product emulation. Prioritize the
physical identity and operating feel; implement only enough honest engine behavior to support the
approved face first, then refine the audio in later increments.

Honor the repository constraints and engineering conventions in `CLAUDE.md`. This skill has a deliberate
equipment-specific execution gate: research and planning are collaborative, but once the user explicitly
approves the equipment Story for autonomous execution, that approval authorizes the listed tasks and one
focused commit per completed task. It does not authorize unlisted scope, architectural work, destructive
operations, or external side effects. Read the relevant settled decisions in `docs/IMPLEMENTATION_PLAN.md`
before planning edits.

## Architectural boundary

Equipment additions are **additive content work**. Use the architecture and extension seams that already
exist; do not make major architectural decisions or redesign shared infrastructure while adding a device.

Ordinary additive work may include:

- A catalog entry composed from existing nodes.
- A new device-specific node that implements the existing `Node` contract without changing it.
- A bespoke faceplate or a narrowly reusable widget using the existing `DeviceUiProps`/handle seams.
- Device-specific tests, styles, tokens, and readout presentation that preserve existing protocols.

Treat work as an architectural gap when an honest implementation would require changing a shared contract
or system shape, including the node/graph/schedule model, signal domains or clock model, catalog descriptor
schema, param/config/readout lanes, scene or worklet protocols, session/patching architecture, workbench
responsibilities, or the UI/engine ownership boundary. A large workaround, hidden conversion, duplicated
framework path, or device-specific exception in shared orchestration is also evidence of a gap, not
permission to proceed.

When a gap appears:

1. Verify it against the landed code and explain why the existing seams cannot implement the equipment
   honestly and additively.
2. Write a concise gap report in the equipment brief: the blocked requirement, current seam, concrete
   limitation, affected files/contracts, and the smallest architectural question that must be resolved.
3. Preserve the equipment research, approved visual scope, and intended resume point.
4. **Stop the equipment task.** Do not choose or implement the architectural solution, smuggle it into a
   device task, or continue with a misleading approximation.
5. Ask the user to solve and review the architectural gap in a separate session/body of work.

Resume the equipment addition only after that architecture has landed and been verified. Re-read the
changed seams, update the equipment brief and task design, obtain the normal program-design approval, and
continue from the recorded resume point.

## Argument

A real device, visual reference, equipment archetype, or related family, for example:

- `LA-2A-style optical compressor`
- `a family of VCA, optical, and FET compressors`

Resolve which revision or era is the main visual reference when products changed substantially between
generations. If it cannot be inferred from the request or sources, ask before fixing the design around
one revision.

## Workflow

### 1. Research the physical identity before proposing code

Browse the internet for current or archived primary sources. Prefer, in order:

1. Manufacturer product pages, manuals, dimensional drawings, and official photographs.
2. Clear retailer pages such as Thomann for dimensions, control inventories, and additional views.
3. Reputable reviews, archived listings, and used-market photographs only to fill gaps.

Use front and rear views when available. Keep citations with the brief and distinguish verified facts,
visual inference, and simulator choices. Do not store copied manuals or product photography in the skill.

Read `references/equipment-brief.md` and fill its relevant sections. Rank the device's identity in this
order:

1. Silhouette, form factor, and dimensions.
2. Major control groups and their relative placement.
3. Signature meters, knobs, switches, and connectors.
4. Faceplate material, color hierarchy, and typography character.
5. Front-versus-rear I/O arrangement.
6. Character-defining operation and interaction.
7. Secondary legends and decoration.

Aim for resemblance rather than reproduction. Use an original device name and altered branding; do not
copy logos or exact trade dress. Preserve the archetype's recognizable visual grammar and workflow.

Present the researched brief and a proposed first increment before editing code. Separate:

- **Must look right now** — dimensions, layout, dominant controls, primary meters and I/O, materials.
- **Must minimally work now** — the smallest valid ports, params, configs, and readouts needed for an
  honest interactive faceplate.
- **Deferred** — nuanced DSP, rare modes, detailed calibration, maintenance controls, and secondary I/O.

Stop for user approval of this scope.

After approval, store the settled brief at `docs/equipment-briefs/<type-id>.md`. Treat that file as the
source of truth for the visual target, first increment, explicit deferrals, citations, and any later
architecture-gap report. Do not begin implementation planning from an unapproved or chat-only brief.

### 2. Plan an executable equipment Story

Cross-check the requested device against the landed code, not only the plans. Determine:

- Stable fictional `typeId` and display name.
- Rackmount or desktop dimensions in millimetres.
- Analog, digital, acoustic, or event domains.
- Mono/stereo and balanced/unbalanced behavior.
- Front and rear connectors.
- Runtime params versus structural configs.
- Required readouts and their units/ballistics.
- Existing engine nodes that can be composed behind one chassis.
- Any additive device-specific node or reusable UI widget needed within existing contracts.
- Whether any requirement exposes an architectural gap under the boundary above.

The Rust catalog is the truth for the exposed face: a bespoke Svelte faceplate binds controls, sockets,
configs, and readings by the IDs derived from its catalog nodes. Therefore a visual-first device still
needs a thin valid model before its interactive controls can render honestly.

Prefer, in order:

1. Compose existing nodes.
2. Add a simple pass-through node in the correct signal domain.
3. Add only the parameter declarations and readouts required by the approved physical face.

A deliberately neutral first increment is acceptable. A wrong carrier domain, fake connector, hidden
converter, or control that appears functional while having no documented behavior is not. Mark any
intentionally shallow control behavior in the task design and deferred list.

If none of the existing seams can express the approved device without changing a shared contract, follow
the architectural-gap stop procedure. Do not draft a device implementation around an unapproved new
architecture.

Read `references/equipment-story.md` and create `docs/equipment-stories/<type-id>.md`. Link it to the
approved brief and break the implementation into dependency-ordered tasks. Size every task so that it:

- Has one coherent outcome and can be understood and executed from the repository plus the two equipment
  documents, without relying on chat history.
- States its dependencies, owned files/shared seams, relevant program design, acceptance checks, and
  focused validation commands.
- Produces exactly one focused commit when complete. Split a task before execution if it cannot remain one
  reviewable commit.
- Leaves the repository compiling and tests relevant to the increment passing.

The task order should preserve the look-and-feel-first gates: thin honest model, physical faceplate,
workbench visual iteration and approval, preapproved good-enough sonic behavior with unit tests, then a
representative end-to-end verification gate with the user. A task may depend on earlier tasks;
“independently executable” means its inputs, scope, done-state, and commit boundary are explicit, not that
tasks can be run in arbitrary order.

For each clear task, include only the call tree, file-tree delta, or key signatures that carry a real
decision. Later task details may be refined just in time, but only within the approved Story. Stop and
present the complete Story/task breakdown for user approval. Ask explicitly whether approval includes
autonomous execution and per-task commits; do not infer commit authorization from approval of the visual
brief alone.

After the user approves autonomous execution, proceed through the listed tasks without requiring the
normal per-task review from `CLAUDE.md`. The approved Story is the separate permission contemplated by
that workflow. Still stop at the visual-approval gate, an architectural gap, a material scope change, a
destructive action, or any new permission boundary.

### 3. Keep device work isolated and parallel-friendly

Prefer standalone, device-owned files so separate equipment Stories can be developed in branches or
worktrees with small merge surfaces:

```text
docs/equipment-briefs/<type-id>.md
docs/equipment-stories/<type-id>.md
crates/devices/src/catalog/<device>.rs
crates/engine/src/node/<device-or-shared-family-node>.rs
web/src/widgets/<Device>.svelte
web/test/faceplates/<device>.test.ts
```

Use the existing `catalog/computer.rs` child-module pattern for device catalog definitions where it fits.
For a parallel batch, first agree and land a common scaffold on the shared base: shared family processing
primitives/widgets and all predictable central module, re-export, catalog, faceplate, skin, focus, and test
registrations. Then give each device task explicit file ownership and keep branch-local work out of shared
registries.

Central aggregation edits that cannot be prewired should be a small, declared integration task with a
defined merge order. If a device branch discovers a new shared prerequisite, stop that branch and handle
the prerequisite centrally; if it changes a shared contract or system shape, use the architectural-gap
procedure. Do not introduce auto-discovery, code generation, or a new registry mechanism merely to avoid
merge conflicts during equipment work.

### 4. Implement the visual slice

Add the thinnest valid Rust catalog/model slice, then concentrate on the bespoke faceplate. Normally
inspect or edit these seams:

- `crates/devices/src/catalog/<device>.rs` — preferred home for identity, dimensions, node expansion, UI
  metadata, ports, configs, and readouts; keep the central `catalog.rs` edit to module/registry aggregation.
- `crates/engine/src/node*.rs` — only for an additive node that fits the existing `Node`/signal/param/
  readout contracts unchanged.
- `web/src/widgets/<Device>.svelte` — bespoke front and rear faces.
- `web/src/device-ui.ts` — faceplate registration and an optional dedicated focus surface.
- `web/src/skin.ts` and `web/src/styles/tokens.css` — skin choices or genuinely reusable visual tokens.
- `web/test/faceplates/<device>.test.ts` — preferred device-owned exposed-face coverage; use the central
  test file only when the current harness requires a small declared aggregation edit.

Compose existing physical-layout and bound widgets before adding one-off markup: `Chassis`, `Row`, `Col`,
`Place`, `Control`, `Socket`, `Reading`, `Legend`, `Led`, and the config controls. Layout is authored at
1 px/mm and scaled by the world/workbench camera. Keep port and control IDs literal where practical so
the static faceplate guardrail can verify them.

Create a reusable widget only when the equipment genuinely introduces a recurring physical control or
display, such as a rotary selector, illuminated pushbutton, stepped control, dual-concentric knob, or
gain-reduction meter. Do not stretch an existing widget until its meaning becomes misleading.

Do not refactor the catalog, workbench, session, patching, or engine architecture as cleanup bundled with
the equipment. Record worthwhile but non-blocking improvements separately and continue through existing
seams. For a blocking improvement, use the architectural-gap stop procedure.

Adding a new device alone does not require a scene schema bump. If the equipment would require new
persisted scene semantics, treat that as an architectural gap and stop. Change `SCHEMA_VERSION` during
equipment work only when following an already-settled existing convention, such as invalidating saved
positions after an approved change to an existing device's exposed IDs; do not design new schema here.

### 5. Iterate in the workbench and obtain visual approval

Build the WASM and run the web development loop described in `CLAUDE.md`. Open:

```text
/devices/<typeId>
```

Read and follow `references/browser-test-loop.md` for this phase. Prefer the built-in browser when it is
available because it provides a shared rendered view, screenshots, element comments, and direct
interaction. Fall back to Computer Use with a connected Chromium browser, then to user-supplied fixed-view
screenshots when no controllable browser is available. Do not claim browser, WASM, audio, or interaction
verification that was not actually observed.

Use the workbench's real-dimensions grid, front/rear rendering, movable supporting gear, patching, meters,
and URL-persisted temporary scene. Add suitable source, converter, and monitor devices from the bench and
patch them manually; do not add workbench-specific vocabulary to the Rust catalog.

Review the face in this order:

1. Overall silhouette, scale, and front/rear proportions.
2. Major grouping and whitespace.
3. Relative knob, meter, switch, and jack sizes.
4. Materials, color, typography, and silkscreen.
5. Interaction, readability, cable clearance, and meter motion.

Use fixed front and rear views at a stated workbench zoom when comparing iterations. After every visual
pass, the primary supervising manager—not the Terra implementer—grades the rendered device from 0 to 5
using the rubric in `references/browser-test-loop.md`. Record the score, evidence, and highest-priority
mismatches in the equipment Story.

If the score is below 4, send concrete corrections to the Terra implementer and continue the visual loop
without asking the user to review an unfinished face. A score of 4 or 5 makes the faceplate eligible for
the user gate; it is not user approval. Present the consistent front/rear evidence, manager score, rubric
reasoning, and known departures, then stop for explicit user approval. Keep the visual task open through
this gate. If the user requests changes, return to the Terra loop and regrade. Mark and commit the visual
task only after user approval. Do not begin detailed DSP merely because the page renders or the manager
score reaches 4.

### 6. Add the good-enough sonic identity

After visual approval, define the smallest audible and measurable behavior that distinguishes the
archetype. Start from what the learner should hear, see on the meters, and understand by operating it.
Prefer a shared processing core plus small device profiles when adding a family.

For example, compressor archetypes might differ mainly in detector timing, knee, control ranges, meter
behavior, and modest coloration:

- VCA — quick, controlled, and firm.
- Optical — slower, softer, and somewhat program-dependent.
- FET — very fast and more aggressive.

Treat these as starting hypotheses to verify against the researched sources, not universal constants.
Do not grow a component-level circuit simulation unless the user explicitly expands the fidelity goal.
Preserve the repository's signal-type split: an analog chassis should not silently become a digital
effect through an invented conversion path.

Implement behavior incrementally. During an approved autonomous Story, refresh the task's program design
and execution record in the equipment Story without requesting another review unless the refinement
changes an approved decision or crosses a stop condition.

Make DSP behavior unit-testable as far as the current architecture permits. Prefer deterministic tests at
the narrowest honest seam: node transfer behavior, control mapping, detector/envelope timing, state
transitions, bypass/power behavior, readout values, channel symmetry, and edge conditions as applicable.
Use hand-calculated numeric oracles for analog behavior and state the calculation in the test. Do not rely
on listening, screenshots, or one broad end-to-end test where a focused unit test can identify the fault.
If an important behavior cannot reasonably be unit tested through existing seams, record why and cover it
at the nearest integration level without changing architecture merely for test convenience.

### 7. Supervise implementation with Terra subagents

For an approved autonomous Story, the primary/orchestrating Codex session is the harness and remains
accountable for scope, architecture, verification, Story state, and commits. Delegate implementation to
Terra-tier subagents (`gpt-5.6-terra` in the current harness) instead of consuming the primary session on
long code-writing stretches. If Terra subagents are unavailable, report that limitation and obtain the
user's direction before silently changing the agreed execution model.

Use this loop for each task:

1. The primary session re-reads the approved brief, Story task, landed code, and current git status. It
   records the task baseline and confirms that dependencies and declared seams still match reality.
2. Spawn one Terra implementation agent with a bounded assignment containing the task outcome, owned
   files, allowed shared seams, program design, acceptance checks, focused test commands, architecture
   stop rule, and the instruction **not to commit**. Give it the brief and Story paths so its work remains
   resumable and grounded in the approved scope.
3. Let that agent implement and run focused checks. It must report changed files, tests run, remaining
   uncertainty, and any suspected architecture or scope gap. Its report is evidence to inspect, not task
   acceptance.
4. The primary session independently reviews the actual diff and relevant surrounding code, checks for
   unrelated/user-owned changes and architectural drift, and reruns the acceptance checks itself. For UI
   work, the primary session also performs the browser/workbench inspection and assigns the rubric score;
   a subagent's screenshot, self-score, or claim does not replace the harness's verification or the user's
   visual approval.
5. If correction is needed within scope, send a precise follow-up to the same Terra agent and repeat the
   review. Do not fix substantial implementation defects in the supervising context or lower the checks
   merely to keep the autonomous run moving. Stop on a material scope decision or architectural gap.
6. Only after independent acceptance does the primary session finalize the Story record, stage the exact
   task diff, and create the task commit.

Do not run multiple implementation agents concurrently in the same working tree. Parallel device work is
allowed only when each agent has an explicitly isolated branch/worktree, standalone file ownership, a
landed common scaffold, and a declared integration order. The primary session must still verify each
result in its integration context before committing or merging it.

### 8. Execute, document, validate, and commit each task

At task pickup, re-read the approved brief, Story, dependencies, and landed code. Update the task marker
to `building` and refine its program design if needed. Keep the Story current while working: record
material decisions, deviations, deferred follow-ups, exact validation performed, and the next resume
point. Do not silently change the task's goal.

When the task meets its acceptance checks:

1. Run its focused validation and any broader gate proportional to the change. For the final task, run
   the full Story gate and prepare the Story-completion record before committing.
2. Update the task to `done` in `docs/equipment-stories/<type-id>.md`, including delivered behavior,
   validation results, remaining shallowness, and the intended commit subject.
3. Review the diff for unrelated or user-owned changes and stage only the task's files/hunks.
4. Create exactly one focused commit for the task using the recorded subject. Do not rewrite or absorb
   pre-existing user changes. Do not add a second metadata-only commit just to record the resulting hash;
   a resumed session can recover it from `git log`.
5. Re-read the next task against the now-landed code and continue autonomously if it remains within the
   approved Story.

For the visual task, manager score 4–5 is only readiness for user review. Do not mark it done or commit it
until the user approves the visuals. DSP tasks remain blocked until that approval is recorded in the Story.

Keep the final DSP/end-to-end task open until joint user verification. The primary manager may build and
preflight the representative workbench patch, run all available automated checks, and prepare exact user
steps and expected audible/visible results. It must then present that scenario to the user and stop. Mark
and commit the task, and mark the Story done, only after the user confirms the end-to-end behavior. If the
joint check finds an in-scope defect, return it to a Terra implementer, rerun unit and integration checks,
and repeat the user gate. A material behavioral change requires renewed scope approval.

If validation fails, keep the task `building`, record the failure and resume point, and fix it within the
task. If the required fix expands scope, touches an undeclared shared seam, or exposes an architectural
gap, stop instead of committing a partial or misleading result.

### 9. Validate the Story

Visual and interaction checks:

- Front and rear match the approved hierarchy at physical scale.
- Every intended control, config, readout, and port is reachable.
- Control travel, detents/steps, labels, and meter motion support the intended operating feel.
- The user approves the faceplate before DSP scope expands.

Structural checks:

- Catalog metadata aligns with actual exposed node faces.
- Bespoke faceplate tests cover all IDs and only valid IDs.
- Connector compatibility, lane count, direction, and signal domain are correct.
- The device can be instantiated and patched in the workbench.

Audio checks:

- Unit-test DSP at the narrowest deterministic seam as far as possible; document any behavior that can
  only be covered at integration level.
- Use hand-calculated numeric tests for analog behavior, with the calculation in the test comment.
- Test the defining static curve, timing, bypass/power behavior, meter reading, channel behavior, and
  relevant edge conditions as applicable.
- Preserve determinism and the no-allocation/no-panic audio-path constraints.
- Include an end-to-end representative patch or render when it tests behavior not covered at node level.

End-to-end acceptance:

- The primary manager preflights one representative device path in the real workbench and records the
  exact patch, controls, actions, and expected audible and visible results.
- Present those steps and the manager's observed evidence to the user for joint verification.
- Do not substitute automated/unit coverage for this user gate, and do not claim acceptance from an
  unobserved or unavailable browser/audio path.
- Do not mark the final DSP task or Story done until the user confirms the end-to-end behavior.

Run focused tests during iteration. Before committing the final task, run the full Rust and web gates and
format web files with the repository's combined formatter. Update the Story with its final validation,
delivered summary, intentionally shallow behavior, deferred researched characteristics, and completion/
resume state, then include that durable record in the final task commit. Report the ordered task commits
and any deferred work to the user.

## Family decisions

Default to separate catalog devices for equipment that should feel distinct in the catalog and on the
bench, while sharing engine primitives and reusable widgets internally. Use a structural config only
when it represents a real build-time option of one chassis, not as a shortcut that hides meaningfully
different products behind one device.

Before implementing a family, agree on the common core, the per-device visual landmarks, the smallest
behavioral differences, and whether each member deserves its own meter/control vocabulary.
