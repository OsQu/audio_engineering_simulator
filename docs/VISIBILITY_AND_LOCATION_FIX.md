# Visibility and location architectural fix

Companion to [PATCHING_AND_VISIBILITY.md](PATCHING_AND_VISIBILITY.md). That document records observed failures and root causes; this document defines the implementation plan.

## Goal

Make one pure, testable model authoritative for:

- where racks and devices are, including live drag previews;
- which device faces are presented in a view;
- which sockets are visible and patchable;
- where cable endpoints and portals attach;
- which objects occlude other objects;
- which rendered target a pointer may select.

Svelte components should render the resolved model and forward gestures. They should not independently reinterpret placement, facing, visibility, or stacking.

The result must fix the current rack, cable, and patch-target visibility failures without replacing the sound engine or faceplate component system.

## Scope

This plan covers the studio wall/top views and the device workbench. It includes the location and visibility parts of patch interaction, including target eligibility and endpoint geometry.

The engine patch-acceptance stall is a separate transaction problem. Workstream T below starts after the test harness is established, independently of the view migration. It must not be coupled to the view resolver.

## Approved product decisions

The user approved all three recommendations below. These decisions are settled implementation requirements; no product-choice gates remain in this plan.

| ID | Approved behavior | Dependent work |
| --- | --- | --- |
| U1 | Suppress affected physical leads during a device/rack flip, retaining a picked cable's logical source and banner; reveal them when settled. | Task 2.1 presentation policy and Phase 4 transition rendering |
| U2 | Use the studio's hidden-face cable exits in the workbench; show a labeled listening badge on the device when its monitored socket is hidden. Hidden sockets remain non-patchable. | Workbench portions of Tasks 2.1 and 4.1 |
| U3 | Place ejected gear in the nearest legal free position to its current effective location, preserving effective room, wall, and facing. Apply the same policy to occupants when removing a rack. | Eject/remove command behavior in Task 3.2 |

No user choice is required for separate toolbars, normalized placement inheritance, duplex socket identity, release-time targeting, or transaction recovery. Focus-window sockets remain excluded from patching in this scope; adding patching there is a separate feature. Top view retains floor footprints with no physical socket targets or cable leads.

## Architectural decisions

### 1. Stored placement and effective placement are different types

The save format remains the durable authoring model. A resolver derives effective location from it.

- A free device derives from its own placement.
- A mounted device derives space, wall, projected position, and facing from its rack plus U-slot.
- A rack drag preview affects the rack and all mounted devices.
- A device preview may resolve as free or as mounted in a candidate rack slot.
- The workbench supplies its own flat placements through the same resolved output contract.

During the migration, redundant `space` and `wall` fields on mounted device placements may remain in persisted scenes for compatibility. Consumers must stop treating those copies as authoritative. A later schema cleanup may remove them.

Keep effective 3-D placement resolution separate from view projection internally, even if one public entry point composes them. Preserve the current rack frame-origin convention used by `rackRect`/`deviceRect`; the U-slot region starts at `FRAME_MARGIN` inside that frame. Fix conflicting comments rather than shifting existing scenes. Missing parent, invalid slot span, missing descriptor, or missing room returns an unresolved record with a diagnostic and no physical target; it never silently falls back to stale free-placement fields.

### 2. The view resolver returns data, not CSS instructions scattered across components

Introduce a pure module, provisionally `web/src/view-resolution.ts`, with an entry point shaped like:

```ts
export function resolveView(input: ResolveViewInput): ResolvedView;
```

The exact names can change during implementation, but the ownership boundary is settled. Its inputs are plain data:

```ts
interface ResolveViewInput {
  scene: Scene;
  descriptors: Readonly<Record<string, DeviceDescriptor>>; // effective per-instance descriptors
  view: StudioView | BenchView;
  preview?: PlacementPreview;
  geometry: DeviceGeometryIndex;
  presentation: PresentationState; // explicit flip states, policies, and stable local attachments
}
```

Its output describes the complete presentation:

```ts
interface ResolvedView {
  objects: ResolvedObject[];
  faces: ResolvedFace[];
  sockets: ResolvedSocket[];
  connections: ResolvedConnection[];
  hitTargets: ResolvedHitTarget[];
  occluders: ResolvedOccluder[];
  diagnostics: ResolutionDiagnostic[];
}
```

All output coordinates use view-local millimetres, x right and y down. Existing world/elevation math stays y-up behind one conversion boundary; use distinct `WorldPoint`, `FaceLocalPoint`, and `ViewPoint` types rather than silently reusing `Rect2` across coordinate spaces. Client coordinates use CSS pixels. The camera adapter supplies a plain invertible affine transform and viewport clip; hit-testing math stays pure.

The composition layer obtains effective instance descriptors using the same resolution as `session.descriptorOf()`, with static type descriptors as fallback. Descriptor/configuration revisions accompany geometry registrations; stale geometry must never be paired with a new port layout.

`PresentationState` contains stage identity, the approved studio/bench policy (U1/U2), and flip records keyed by object ID: generation, from-face, and to-face. Suppressed leads retain their logical endpoint references; storing frozen cable routes during a flip is unnecessary. It contains no DOM references or clock reads. Start, finish, cancel, and interrupted-flip events update it outside the resolver. Rack flip state applies to its occupants. Only the current generation can finish a transition.

Held cables are a separate pure derivation, avoiding a dependency from the scene resolver to the patch controller:

```ts
resolveHeldCable(view: ResolvedView, pick: HeldCableInput): ResolvedHeldCable;
// pick supplies logical source/socket identity, pointer in view coordinates,
// optional current target, and gesture mode. It never supplies a cached world anchor.
```

The initial object/face layout must resolve even when geometry is empty: render those faces, register their local socket geometry, then resolve sockets and leads. This prevents a circular dependency where a face must be measured before it can render.

### 3. Face identity is explicit

`effectiveFacing(deviceId)` is too small to describe every presentation. The studio presents one effective face; the workbench may present front and back simultaneously. Each rendered face therefore has its own identity:

```ts
type FaceInstanceId = string;

interface ResolvedFace {
  id: FaceInstanceId;
  objectId: string;
  face: "front" | "back";
  rect: ViewRect;
  state: "presented" | "transitioning" | "hidden";
  interaction: "enabled" | "disabled";
}
```

Socket identity includes the face instance that presents it. Duplicate faceplates in focus windows cannot accidentally borrow anchors or become world patch targets.

### 4. Socket geometry is device-local input

The model needs a socket's owning face and local position. It must not infer physical truth from whichever transformed DOM node happens to be under the pointer.

Adopt this in two stages:

1. Measure rendered sockets in their untransformed face-local layout and publish a plain `DeviceGeometryIndex` per face instance and layout revision. This retains existing faceplate markup while removing camera, placement, and flip transforms from the measurements.
2. Where practical, let the faceplate layout primitives publish authored local geometry directly. Do not duplicate hand-maintained socket coordinates beside existing layout code.

DOM measurement remains an adapter and receives small browser tests. All projection, face selection, endpoint resolution, and hit eligibility after that boundary are pure.

Measurement contract:

- Origin is the outer faceplate chassis top-left before flipping, in mm at natural scale; include bezel/padding offsets exactly once. Record face dimensions and physical socket centre/radius.
- Read untransformed layout offsets through the face's layout ancestors, including borders and scroll offsets. Do not subtract transformed `getBoundingClientRect()` values or attempt to invert an edge-on flip rectangle. Internally transformed socket layouts must provide their local transform or authored geometry through the same registration API.
- Keep hidden faces laid out inside the existing faceplate instance, with paint and interaction disabled. Do not create an extra live device/control tree just for measurement. Workbench duplicate front/back instances register only their assigned face; the two instances together provide both faces.
- Mount, descriptor/layout revision, face size, font readiness, and explicit content-layout invalidation schedule one measurement batch. `ResizeObserver` alone cannot detect a socket moving within an unchanged outer box. Unmount unregisters that face instance. Ignore results for obsolete revisions/generations.
- Cache per face instance initially. Sharing measurements is optional and requires matching type, config/descriptor revision, natural dimensions, layout/skin revision, and font metrics. Instance differences must not overwrite each other.
- Empty geometry produces `geometry-pending` with disabled targets and suppressed unresolved leads. View membership can still resolve without geometry; it must not be mistaken for off-view membership.

Model a physical socket separately from its logical ports: `{ socketId, faceInstanceId, localCentre, radius, endpoints: PortEndpoint[] }`. A duplex USB socket has two endpoint aliases but one hit target; preserve its canonical output endpoint and paired input mapping. A persisted connection endpoint resolves through that alias table. Tests must exercise USB in both gesture directions and after descriptor changes.

### 5. Visibility and interaction are separate properties

A socket or cable end can exist without being painted or selectable. Resolve these states explicitly rather than overloading “anchor missing”:

```ts
interface SocketPresentation {
  membership: "local" | "off-view" | "unresolved";
  faceState: "presented" | "hidden" | "transitioning";
  geometry: { kind: "known"; localPoint: FaceLocalPoint }
    | { kind: "pending" };
  exposure: "clear" | "partial" | "occluded" | "not-presented";
  patchable: boolean;
}
```

These properties are independent: an off-view socket can have known geometry, and a presented socket can be occluded by a different device. Patchability requires local membership, presented face, current geometry, and an exposed centre. Partial sockets with an exposed centre remain eligible, but their hit region is clipped to the exposed area. A hidden socket may still resolve a cable exit; that exit belongs to the **connection route**, not to the socket. An off-view endpoint supplies a view address for a portal. Invalid references produce diagnostics and no targets rather than mixing fallback locations.

### 6. Stable physical layers; controls live in a UI overlay

Hover and keyboard focus must never alter the stacking of a chassis, rack, mounted device, or cable. The resolver assigns stable physical paint groups. Suggested semantic layers are:

```ts
type PaintLayer =
  | "furniture"
  | "rear-object"
  | "cable"
  | "front-object"
  | "cable-foreground"
  | "interaction";
```

The names express semantics; the renderer maps them to CSS/SVG layers. Device/rack toolbars render in the interaction overlay and refer to their object by ID. Their hit box covers the controls only. Dragging may add a visual treatment, but it does not arbitrarily raise an opaque chassis above unrelated physical layers.

The layer names alone are insufficient. Implement this deterministic paint/mask contract:

1. Furniture paints behind devices. Device order is `(physical layer, stable scene order, object ID)`; a later item in this total order covers an earlier one. Preview does not reorder objects. Rack parentage determines transforms, not a DOM stacking context that traps occupants behind the frame.
2. Every chassis supplies a view-local rounded-rectangle silhouette matching the renderer's outer clip. The initial model treats it as opaque, including controls; transparent chassis are outside this scope. The resolver emits silhouette IDs, total order, and which silhouettes mask each cable pass.
3. The main cable pass paints above rear objects and below front objects. Its mask additionally removes the silhouette of any endpoint chassis whose connected socket is on a hidden face. This prevents a curved lead reappearing inside that chassis.
4. A cable attached to a presented front socket gets a foreground pass clipped to that owner's silhouette, **minus every device silhouette ordered above the owner**. Thus a lead can reach its front socket without painting across a different foreground device. Rear endpoints need no owner redraw. Apply these passes to ordinary cables and portal leads identically.
5. Foreground passes and the main pass use the same path geometry. SVG consumes the resolver's explicit include/exclude silhouette sets; components do not invent masks. For cable selection, use the same pass visibility predicate so invisible segments cannot receive hits.
6. Socket exposure uses the same object order and silhouettes. UI windows and controls additionally block pointer hits in the browser adapter; geometric tolerance must not reach through them.

For hidden-face routing, choose the nearest chassis edge to the canonical local socket position, with tie order bottom, right, left, top. Route outward through a short outward-normal lead before joining the sagging cable. Keep the hidden owner masked along the entire curve; if paths cross it again they remain invisible there. Edge choice is independent of the other endpoint, preventing portal movement from arbitrarily swapping sides. If a foreground chassis covers the exit, mask it rather than moving the socket or inventing a new connection. Tests cover a front-socket lead crossing another front device, crossing a rear device, overlapping endpoint chassis, and portals under the same conditions.

This policy applies to both the studio and workbench under approved decision U2. Replace the workbench's mirrored hidden attachments with these exits, and replace its hidden-socket listening marker with a labeled device badge. Hidden sockets remain disabled as patch targets.

### 7. Ordinary cables, portals, and held cables share endpoint resolution

Every cable-like presentation begins with the same resolved endpoint state. The connection resolver then chooses:

- full cable when both endpoints are local;
- portal cable when exactly one is local;
- no presentation when neither is local;
- held cable from its resolved source to the pointer or target.

Hidden-face exits, chassis masking, transition behavior, and target eligibility are shared. Portal stubs cannot bypass the rules used by ordinary cables.

### 8. Hit testing consumes the resolved view

Pointer press, move, and release use one target resolver. It searches only `ResolvedView.hitTargets`, in client-pixel space after applying the current camera transform.

Each socket gets a minimum screen-space hit radius independent of its visual diameter. Occluded, hidden, transitioning, focus-window, and off-view sockets are absent or disabled. Release always resolves the current target and recomputes legality; it never commits solely from a cached pointer-move verdict.

Start with radius `max(8 CSS px, rendered socket radius)` for mouse/pen and `max(12 CSS px, rendered socket radius)` for touch. Keep these named presentation constants for browser tuning. Clip tolerance by viewport and foreground blockers. Prefer an exact physical socket hit, otherwise nearest eligible centre, then paint order and stable socket ID for ties. Do not skip an illegal nearest socket to silently connect to a farther legal one; report the nearest socket's verdict.

Gesture arbitration happens synchronously in a shared stage capture-phase pointer-down adapter, **before** device-drag or camera handlers capture or mutate anything. It classifies the press as control/chrome, socket, visible cable, device move, or backdrop pan, in that priority order. All gesture adapters accept this ownership decision; remove their independent competing pointer-down starts. Controls and focus windows block geometric socket search behind them. Pending picks survive toolbar/view navigation without claiming the toolbar gesture.

Device movement and patching both track one primary pointer. Separate successful release from `pointercancel`, lost capture, blur, and unmount. Device cancellation discards preview; it must not call the current shared `onUp` commit handler. Ignore unrelated pointer IDs. After normal release, clear ownership before releasing capture so subsequent `lostpointercapture` cannot undo a committed gesture.

## Target dependency direction

```text
scene-store + catalog + transient preview + local face geometry
                              |
                              v
                    effective placement
                              |
                              v
                       view resolution
                 /            |            \
                v             v             v
         device renderer  cable renderer  patch target resolver
                \             |             /
                 \            v            /
                    DOM event adapters
```

`view-resolution.ts` may depend on pure spatial/catalog/scene types. It must not import Svelte, browser DOM types, camera state, or patch-controller state.

## Delivery plan

Each task should leave the simulator usable. Do not combine the entire migration into one rewrite.

Execution order: Phase 0 → Workstream T (restore reliable patch validation) → Phase 1 → Phase 2 → Phase 3 → Phase 4 → Phase 5. Workstream T has no dependency on placement/visibility and may run independently once its harness exists. Task numbering below preserves the architectural grouping, not permission to skip dependencies.

Pure preview placement is delivered in Task 1.2, renderer assembly integration in Task 3.2, cable-follow integration in Phase 4, and target-follow integration in Phase 5. Do not claim all four are complete in Task 1.2. The corresponding browser regressions graduate to required tests at the phase that implements them.

### Phase 0 — Lock behavior and create an integration seam

#### Task 0.1 — Add browser regression coverage for confirmed failures

Add the smallest supported browser/component harness needed to exercise actual DOM hit testing and stacking. Cover:

- mounting a rack device and hovering the empty rack without hiding the occupant;
- front and rear device hover without hiding a connected cable;
- click + click and drag + drop reaching the same destination;
- release over a socket without a final pointer-move event;
- socket targeting at representative minimum and maximum zoom;
- moving a populated rack with its occupants visible.

Use Playwright against Vite with isolated browser contexts and unsaved deterministic fixtures. Keep geometry/gesture fixtures independent of the real audio engine using a test session adapter; add a separate real-worklet patch acceptance smoke test. A missing final move event is a deliberate adapter-level event-sequence test, supplemented by normal browser gestures.

Record initial failures in an explicitly named known-regression suite, separate from the required passing suite. Each case names its implementing task; run and report that suite during migration. Move each case into the required suite as its fix lands. No blanket retries, permanent skips, or suppression of new failures. Completion requires an empty known-regression list. Avoid screenshot-only assertions where semantic DOM/layer/geometry assertions are clearer.

#### Task 0.2 — Characterize presentation rules as pure fixtures

Create table-driven fixtures for:

- free and mounted devices on each wall;
- rack front/back facing;
- studio top view;
- workbench single-face support devices and simultaneous DUT faces;
- local/local, local/remote-wall, local/remote-room, and remote/remote connections;
- visible-face and hidden-face sockets.

Record expected effective placement, face instances, socket presentation, and connection presentation. These become the contract for the resolver.

### Phase 1 — Effective placement hierarchy

#### Task 1.1 — Introduce pure effective placement resolution

Add a discriminated stored-location reader and a resolved placement type. Centralize:

- rack inheritance of space, wall, facing, and position;
- U-slot positioning;
- free-device projection;
- top/elevation projection;
- workbench flat placement.

Migrate `deviceRect`, `placedItemsFor`, `effectiveFacing`, cable `inView`, and rack occupant lookup to consume this result. During this task, preserve existing public helpers as thin wrappers where that keeps the diff reviewable.

Acceptance:

- no consumer reads a mounted device's copied `space`, `wall`, or `facing` as the effective truth;
- all existing spatial and placement tests pass with new mounted-device inheritance cases;
- corrupt/missing rack references resolve explicitly and do not silently mix rack and device state.

#### Task 1.2 — Make previews first-class resolver input

Define `PlacementPreview` for a free device, rack, or candidate rack mount. Move transient preview ownership high enough that view resolution can consume it.

Acceptance:

- previewing a rack move translates the rack and every mounted device;
- the pure preview resolves new chassis and face transforms for connected devices; cable-renderer integration is Task 4.1;
- cancel restores the committed resolution without scene mutation;
- commit produces the same resolved geometry as the final preview.

### Phase 2 — Faces and local socket geometry

#### Task 2.1 — Add explicit face instances

Resolve studio and workbench faces into `ResolvedFace` records. Migrate face rendering and patch eligibility away from ad hoc `effectiveFacing` comparisons.

Implement approved flip behavior U1 as follows:

- the outgoing and incoming face are `transitioning` during the animation;
- transitioning sockets are not patch targets;
- suppress each physical lead touching a transitioning device, including portal and held leads; retain any pick's logical source and banner, then re-resolve and reveal the lead when settled;
- transition completion updates the resolver through an explicit event, with no correctness dependency on a 480 ms timer.

Handle `transitionend`, `transitioncancel`, rapid flip reversal, and unmount with a generation token. Zero-duration/reduced-motion transitions settle synchronously after layout rather than waiting for an event that will never fire. Face geometry remains local and independent of the transition. Tests pass explicit transition states into the pure resolver.

Top view emits footprints for racks and free devices, no presented faces/socket targets/cable passes, and no mounted device faceplates. A pending pick may survive top-view navigation as a banner, with no physical lead until returning to a patchable view. Workbench DUT faces are both presented; supporting devices and the listening marker follow U2. The marker references a physical socket through its endpoint alias, so it cannot create an extra patch target.

Acceptance:

- the workbench DUT has two independently addressed presented faces;
- a studio or supporting workbench device presents exactly one stable face outside transition;
- focus-window face instances cannot collide with world/bench identities.

#### Task 2.2 — Replace world-space jack sweeps with local geometry registration

Refactor `jack-anchors.ts`, `Jack.svelte`, `Chassis.svelte`, and the stage adapters so measurement produces device-local, face-scoped geometry. Add resize/layout invalidation using explicit component lifecycle or `ResizeObserver` where necessary.

Implement the registration contract in architectural decision 4, including initialization with empty geometry, hidden faces, per-instance descriptor changes, aliasing, stale measurement rejection, and unregister. Browser-check a non-unit zoom, both flip states, duplicate workbench faces, and two differently configured instances of one device type.

Acceptance:

- pan, zoom, placement, and rack movement do not require remeasuring unchanged local socket geometry;
- the same socket on separately rendered face instances resolves independently;
- temporary absent geometry is represented as `geometry-pending`;
- no hardcoded flip-settle timeout is needed for correctness.

### Phase 3 — Resolved presentation and stable layering

#### Task 3.1 — Produce `ResolvedView.objects`, `faces`, and semantic paint layers

Have WorldView and BenchStage render resolved objects and faces. Move hover/focus chrome into a separate overlay keyed by object ID. Remove the `openChromeId ? 6` and dragging `? 10` chassis z-index overrides.

Acceptance:

- a rack remains behind all mounted devices during hover, focus, toolbar grace time, and drag;
- front/rear cable occlusion does not change on device hover;
- toolbars remain usable even when wider than the chassis;
- the physical layer order is exhaustively unit tested as semantic output and minimally browser tested as actual stacking.

#### Task 3.2 — Render rack assemblies from one preview transform

Represent rack-to-occupant parentage in resolved objects. Ensure drag affordances and legality feedback attach to the assembly without covering its children.

Acceptance:

- the rack frame and occupants consume the resolved preview transforms; Phase 4 then connects cable geometry and Phase 5 connects socket targeting;
- occupant controls and sockets remain interactable when the rack is stationary;
- rack removal/ejection leaves explicit, valid free placement.

Implement approved decision U3 by computing all affected free placements before mutating the scene. Search the existing free grid by increasing squared distance from the effective projected position, tie-breaking by y then x, excluding other gear and allocating multiple occupants in slot order. Preserve effective room/wall/facing; if no legal location exists within the room, report failure and keep the operation atomic. Do not restore remembered free positions. Removing a rack must resolve its children before deleting their parent; exclude the removed frame from candidate collision checks, but include a retained rack when ejecting a single device.

### Phase 4 — Unified cable visibility

#### Task 4.1 — Resolve cable endpoints from socket presentation

Replace `CableLayout` callbacks such as `clampsEstimate` and `frontPatchOver` with resolved endpoint data. Define a physical exit for a hidden-face cable using the actual local socket position and chassis boundary/mask policy.

Implement the exact ordering, edge selection, and mask-set contract in architectural decision 6. The renderer uses SVG masks/clips, while the resolver decides all pass geometry and include/exclude silhouettes. Cable hit testing evaluates the same visibility predicate. Do not compute cable visibility from incidental CSS z-index values. This task completes the cable-follow acceptance deferred from Task 1.2, using current preview transforms for endpoints and every clip.

Acceptance:

- each visible-face cable ends at its actual socket;
- a hidden-face cable emerges from an intentional chassis edge and never ends visibly at an interior estimate;
- sagging curves cannot re-enter an opaque chassis after their chosen exit;
- overlapping chassis cases have deterministic, tested behavior.

#### Task 4.2 — Move portals onto the same endpoint pipeline

Build full cables, portal stubs, and portal chips from `ResolvedConnection`. Preserve persisted chip offsets, but apply them after resolving the local physical endpoint.

Acceptance:

- moving the remote device between walls/rooms changes full cable to portal while preserving local socket identity/position and the defined hidden-face exit rule; the path and its masks may change;
- front/rear and hidden/visible rules are identical for ordinary and portal cables;
- portal interaction UI remains above physical content while the lead follows physical occlusion.

#### Task 4.3 — Move held cables onto the same pipeline

Represent the held source as a resolved endpoint reference, not a stale absolute `srcPoint`. Re-resolve it after movement, rack movement, view change, and flip completion.

Acceptance:

- held cables follow their source device and rack;
- a view switch produces the correct floating/portal state;
- flipping the source follows the defined transition policy;
- cancel always removes only transient interaction state.

### Phase 5 — Unified patch targeting and gesture lifecycle

#### Task 5.1 — Add geometry-based socket hit targets

Derive eligible socket targets from `ResolvedSocket`. Convert them to client coordinates through a small stage adapter and apply a documented minimum pixel radius. Resolve overlaps by the exact-hit, distance, and paint-order rules in decision 8. This task delivers the pure target classifier; the near-socket browser acceptance below is completed jointly with Task 5.2's arbitration integration.

Acceptance:

- near-socket presses do not accidentally start a device move;
- hidden and occluded sockets cannot be selected;
- target behavior is stable across zoom levels;
- focus-window sockets follow an explicit policy rather than inheriting world anchors accidentally.

#### Task 5.2 — Give patch gestures explicit pointer ownership

Refactor `PatchController` to track pointer ID, primary button, capture/cancel, movement threshold, and current resolved target. Press, move, and release share the same resolver. Re-evaluate the destination and legality on release for drag and click modes.

Integrate the shared capture-phase arbitration from decision 8 with `device-drag.ts`, the camera, and both stage roots. In particular, remove the old device-first capture path and split move completion from cancellation. The expanded socket radius cannot be considered implemented while device dragging can begin first on that area. Test control precedence, toolbar blockers, near-socket presses, multi-pointer input, blur, and cancel as well as patch gestures.

Acceptance:

- drag + drop and click + click produce the same connection verdict;
- release does not depend on the last move event;
- pointer cancel/lost capture returns to a defined state;
- panning, moving gear, operating controls, selecting cables, and patching are mutually exclusive gestures;
- illegal target reasons stay visible so the user can choose another destination.

### Workstream T — Patch transaction reliability, kept outside the view model

#### Task T.1 — Model topology submission as an acknowledged transaction

Replace the boolean `submitPatchCandidate` contract with explicit idle/submitting/accepted/rejected/unknown states. Preserve the user's endpoints while busy or rejected. Surface status beside the gesture.

Associate each request with a request ID, base scene revision, and proposed patch delta. Do not replace the whole current scene with an old snapshot when an acknowledgement arrives.

Use one serialized structural transaction channel for connect, disconnect, cable-type change, device/config changes, and load/reload. Busy actions return an explicit retryable result; do not enqueue stale whole-scene snapshots. Track an engine generation and topology revision separately from UI placement changes. The proposal includes derived changes such as USB enumeration and affected track configuration; acceptance applies that scoped change while preserving newer unrelated placement, parameters, and UI state. Reapply current runtime parameter/track state after the new schedule is installed.

Use a named 5-second acknowledgement deadline initially. Timeout or processor failure enters `unknown`: disable further topology submissions, retain the picked endpoints, and offer explicit engine recovery. Recovery invalidates the old generation, recreates the engine from the last authoritative accepted patch plus current parameter state, and resumes submissions only after the readiness handshake. A timeout is never treated as proof of rejection. Log generation, request ID, base revision, send/reply timestamps, and protocol/build version. Define acknowledgement as compiler acceptance; use the post-install notification to reapply runtime state rather than assuming acceptance already means installation.

Test with a fake transport and clock: acceptance, compiler rejection, busy, unavailable engine, dropped response, late response after recovery, placement/parameter edits during validation, and two structural actions. Add a real-worklet smoke test for acceptance/rejection and protocol mismatch. This workstream is delivered early so browser patch tests can distinguish gesture failures from engine transport failures.

Acceptance:

- busy submission cannot silently discard a held cable;
- compiler rejection reports its reason and permits retry/cancel;
- missing acknowledgement reaches a recoverable unknown state;
- load/restart invalidates obsolete requests, and late replies cannot restore old placement or UI data;
- structural changes are serialized or deliberately rebased;
- worklet/UI protocol versions are checked at readiness.

## File-direction sketch

Expected additions and responsibilities:

```text
web/src/effective-placement.ts   stored placement + preview -> effective hierarchy/location
web/src/view-resolution.ts       effective placement + faces + geometry -> ResolvedView
web/src/socket-geometry.ts       device-local geometry types/index and pure transforms
web/src/view-hit-testing.ts      resolved targets + camera adapter -> current target
```

Expected migrations:

```text
spatial.ts                       retain low-level projection and rectangle/rack math
projection.ts                    become wrappers or fold into effective-placement/view-resolution
placement.ts                     produce commands/previews using effective placement
cable-view.ts                    consume resolved endpoints; lose CSS-workaround callbacks
jack-anchors.ts                  become local geometry DOM adapter
WorldView.svelte                 render ResolvedView; own camera/DOM only
BenchStage.svelte                render ResolvedView with bench policy
App.svelte                       compose session, resolver inputs, renderer, and UI chrome
PatchController                  gesture state + transaction request; no DOM-wide scene interpretation
```

Do not force these filenames if a smaller coherent module split emerges. Preserve the dependency direction and ownership boundaries.

## Testing strategy

### Pure tests

Use compact fixture builders and table-driven matrices. Assert semantic values rather than snapshots of huge objects.

- effective placement inheritance and corrupt-reference handling;
- all studio walls, top view, rooms, racks, slots, and previews;
- studio and workbench face instances;
- socket local-to-view transforms and presentation states;
- object paint layers and occlusion decisions;
- local/local and cross-view connection presentation;
- hidden-face chassis exits and curve/mask decisions;
- target eligibility and overlap selection;
- patch state transitions and transaction revision handling.

Prefer invariants where the state space is broad:

- a mounted device's effective space/wall/facing always equals its rack's;
- preview commit geometry equals committed geometry;
- only presented, enabled sockets appear in hit targets;
- changing hover/focus state cannot change physical layers;
- a local physical socket's identity and position are invariant when only its remote endpoint changes view membership; hidden-face edge selection follows its explicit local rule, while cable curves and masks can change;
- identical input data, including explicit transition state and geometry revision, produces identical resolution without DOM or clock access;
- a duplex physical socket has one hit target and resolves both endpoint aliases;
- no visible cable pass or hit region leaks through a foreground occluder;
- device `pointercancel` cannot commit a preview, and stale flip completion cannot settle a newer flip.

### Browser/component tests

Keep these focused on facts pure tests cannot prove:

- faceplate geometry registration reports the visual socket center;
- CSS/SVG layers implement semantic paint order;
- pointer capture and cancel events reach the controller;
- minimum screen-space socket targets behave under camera zoom;
- flip completion invalidates geometry at the right time;
- rack occupants remain visible and interactive on hover/drag;
- cable masks visually meet device silhouettes.

Use a few targeted screenshots only for final cable/rack visual contracts where geometry assertions do not capture readability.

### Manual browser matrix

Before declaring the work complete, verify the stock scene and a minimal constructed scene at minimum, default, and maximum zoom:

- free and rack-mounted devices, front and rear;
- all four wall elevations and top view;
- same-wall, different-wall, and different-room connections;
- ordinary, duplex USB, and adapter-supported connections;
- device move, rack move, mount, eject, flip, view change, save/load, and engine restart;
- click + click, drag + drop, cancel, illegal target, rejection, and timeout recovery.

## Migration constraints

- Keep the current scene schema readable throughout the migration. If persistence changes, bump `SCHEMA_VERSION` deliberately and document whether old disposable scenes are discarded.
- Do not change engine patch semantics as part of view resolution.
- Preserve existing faceplate components and their bound controls.
- Avoid a second independently maintained socket-layout source.
- Do not expose Svelte proxies or DOM elements through pure model APIs.
- Remove old branches and timers once their consumers migrate; temporary adapters must be named and tracked for deletion.
- Keep performance proportional to visible scene size. Cache device-local geometry, but derive resolved view data deterministically from explicit inputs.

## Definition of done

The architectural fix is complete when:

- one pure resolver answers effective placement, presented faces, socket presentation, connection presentation, and target eligibility;
- studio and workbench renderers consume that output without re-deciding physical visibility or stacking;
- committed and preview geometry share the same path;
- racks and mounted devices behave as a hierarchy;
- ordinary, portal, and held cables share endpoint resolution;
- hover/focus controls do not alter physical occlusion;
- drag + drop and click + click share target and release semantics;
- patch validation cannot silently lose an action or block forever;
- pure tests cover the rule matrix, browser tests cover DOM/layer/event seams, and the manual browser matrix passes;
- the confirmed failures in `PATCHING_AND_VISIBILITY.md` are closed with regression coverage.

## Explicit non-goals

- replacing Svelte or the SVG cable renderer;
- moving room/rack data into the Rust audio engine;
- building a general-purpose 3-D scene graph;
- redesigning device faceplates;
- physically simulating cable collision, routing, or gravity;
- preserving arbitrary old implementation seams solely to minimize the refactor.
