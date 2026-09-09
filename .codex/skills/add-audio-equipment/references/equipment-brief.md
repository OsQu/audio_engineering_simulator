# Equipment brief

Use only the sections that affect the device. Keep the brief concise, cite claims close to their source,
and label uncertain visual readings as inference. After the user approves the researched scope, store the
settled brief at `docs/equipment-briefs/<type-id>.md`; do not leave the authoritative version only in chat.

## Inspiration and boundary

- Reference device or archetype:
- Exact revision/era:
- Original simulator name and `typeId`:
- What may resemble the reference:
- What must remain original:

## Sources

| Source | What it establishes | Confidence |
| --- | --- | --- |
| Manufacturer manual/page |  | verified |
| Retailer/dimensional listing |  | verified or conflicting |
| Front/rear photographs |  | observed/inferred |

Record conflicts instead of silently choosing a convenient value.

## Physical identity

- Form factor and dimensions:
- Front/rear silhouette:
- Dominant visual landmarks:
- Control groups, left to right:
- Meter/display type and behavior:
- Materials, palette, and typography character:
- Front I/O:
- Rear I/O:
- Details that can be omitted without losing the identity:

## Operating identity

- Intended signal domain and channel shape:
- Balanced/unbalanced and connector behavior:
- Primary user workflow:
- What the learner should hear:
- What the learner should see:
- What the learner should understand:
- Bypass, power, sidechain, or routing semantics:

## First increment

### Must look right now

- 

### Must minimally work now

- 

### Explicitly deferred

- 

## Code fit

- Existing nodes to compose:
- Additive device-specific node, if needed:
- Existing UI widgets to reuse:
- New reusable widget, if justified:
- Catalog ports/params/configs/readouts:
- Likely files and guardrail tests:
- Scene schema impact: none, or architectural gap requiring a separate session

## Delivery handoff

- Equipment Story: [`../equipment-stories/<type-id>.md`](../equipment-stories/<type-id>.md)
- Standalone device-owned files:
- Common-base prerequisites and prewired registries:
- Shared family core/widgets that must land before parallel device work:
- Device branch/worktree ownership:
- Declared central integration task and merge order:
- Shared files device-local tasks must not edit:

## Architecture fit

- Can the approved equipment be implemented additively through existing seams? Yes / No
- Existing seams used:
- Shared contracts that remain unchanged:
- Non-blocking improvements recorded for later:

If **No**, complete this gap report and stop the equipment task:

- Blocked equipment requirement:
- Current seam and concrete limitation:
- Evidence from landed code:
- Affected shared files/contracts:
- Why a device-local workaround would be misleading or duplicative:
- Smallest architectural question for the separate session:
- Preserved research/visual decisions:
- Exact equipment resume point after the architecture lands:

## Workbench acceptance

- Browser path used (built-in / connected Chromium / screenshots):
- WASM and AudioWorklet smoke-test result:
- Browser viewport, browser zoom, and workbench zoom:
- Reference front view and fixed zoom:
- Reference rear view and fixed zoom:
- Proportion/grouping checks:
- Interaction and meter-motion checks:
- Supporting devices and representative patch:
- Console/network issues or unverified capabilities:
- User visual approval:

## Later sonic increments

For each increment, state one audible or measurable distinction, its educational purpose, and its test
oracle. Avoid an undifferentiated “improve realism” task.

1. 
2. 
3. 
