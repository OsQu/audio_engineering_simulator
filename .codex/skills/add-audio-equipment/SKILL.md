---
name: add-audio-equipment
description: Research, design, and add new audio equipment to this repository with a look-and-feel-first workflow using the device workbench. Use when adding a gear-inspired device or family of devices; do not use for generic engine nodes or unrelated UI widgets.
---

# Add Audio Equipment

Add recognizable, educational audio gear without making a 1:1 product emulation. Prioritize the
physical identity and operating feel; implement only enough honest engine behavior to support the
approved face first, then refine the audio in later increments.

Honor the repository workflow in `CLAUDE.md`: create tasks, draft the current task's program design,
stop for user review before implementation, validate completed work, and never commit unless explicitly
asked. Read the relevant current Story and settled decisions in `docs/IMPLEMENTATION_PLAN.md` before
planning edits.

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

### 2. Draft the task's program design

Cross-check the requested device against the landed code, not only the plans. Determine:

- Stable fictional `typeId` and display name.
- Rackmount or desktop dimensions in millimetres.
- Analog, digital, acoustic, or event domains.
- Mono/stereo and balanced/unbalanced behavior.
- Front and rear connectors.
- Runtime params versus structural configs.
- Required readouts and their units/ballistics.
- Existing engine nodes that can be composed behind one chassis.
- The smallest missing node or reusable UI widget, if any.

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

Draft or refresh the task's program-design entry in the active Story companion file. Include only the
call tree, file-tree delta, or key signatures that carry a real decision. Stop for user approval before
implementation.

### 3. Implement the visual slice

Add the thinnest valid Rust catalog/model slice, then concentrate on the bespoke faceplate. Normally
inspect or edit these seams:

- `crates/devices/src/catalog.rs` — identity, dimensions, node expansion, UI metadata, ports, configs,
  and readouts.
- `crates/engine/src/node*.rs` — only when the exposed face cannot be represented honestly with existing
  nodes.
- `web/src/widgets/<Device>.svelte` — bespoke front and rear faces.
- `web/src/device-ui.ts` — faceplate registration and an optional dedicated focus surface.
- `web/src/skin.ts` and `web/src/styles/tokens.css` — skin choices or genuinely reusable visual tokens.
- `web/test/faceplate.test.ts` — exposed-face coverage for bespoke surfaces.

Compose existing physical-layout and bound widgets before adding one-off markup: `Chassis`, `Row`, `Col`,
`Place`, `Control`, `Socket`, `Reading`, `Legend`, `Led`, and the config controls. Layout is authored at
1 px/mm and scaled by the world/workbench camera. Keep port and control IDs literal where practical so
the static faceplate guardrail can verify them.

Create a reusable widget only when the equipment genuinely introduces a recurring physical control or
display, such as a rotary selector, illuminated pushbutton, stepped control, dual-concentric knob, or
gain-reduction meter. Do not stretch an existing widget until its meaning becomes misleading.

Adding a new device alone does not require a scene schema bump. Bump `SCHEMA_VERSION` only when persisted
scene semantics or existing positional IDs change.

### 4. Iterate in the workbench and obtain visual approval

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

Use fixed front and rear views at a stated workbench zoom when comparing iterations. Present the result
and stop for explicit user approval of the look and feel. Do not begin detailed DSP merely because the
page renders.

### 5. Add the good-enough sonic identity

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

Implement behavior incrementally, with program-design review before each task as required by
`CLAUDE.md`.

### 6. Validate each increment

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

- Use hand-calculated numeric tests for analog behavior, with the calculation in the test comment.
- Test the defining static curve, timing, bypass/power behavior, and meter reading as applicable.
- Preserve determinism and the no-allocation/no-panic audio-path constraints.
- Include an end-to-end representative patch or render when it tests behavior not covered at node level.

Run the focused tests during iteration and the full Rust and web gates before handoff. Format web files
with the repository's combined formatter. Report what was verified, what remains intentionally shallow,
and which researched characteristics are deferred. Do not commit.

## Family decisions

Default to separate catalog devices for equipment that should feel distinct in the catalog and on the
bench, while sharing engine primitives and reusable widgets internally. Use a structural config only
when it represents a real build-time option of one chassis, not as a shortcut that hides meaningfully
different products behind one device.

Before implementing a family, agree on the common core, the per-device visual landmarks, the smallest
behavioral differences, and whether each member deserves its own meter/control vocabulary.
