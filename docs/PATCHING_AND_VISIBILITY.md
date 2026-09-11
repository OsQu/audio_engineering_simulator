# Patching, cable visibility, and rack rendering assessment

Assessed 2026-09-11 against working tree based on `b7b8a21` (`Fix cyclic dependency check in web`). This is a diagnosis and proposed repair plan; no implementation fixes were made.

## Findings at a glance

There are several independent failures. Fixing only jack hit testing will not fix the transaction stall or the rendering problems.

| Priority | Finding | Evidence |
| --- | --- | --- |
| P0 | A pending engine validation can block every subsequent connection, while the pointer UI discards each attempted cable | Reproduced in the existing Chrome session; controller/session code confirms the failure mechanism |
| P0 | Hover raises an opaque rack above its mounted devices | Reproduced with a newly mounted Luma Leveler |
| P1 | The same hover elevation puts device panels above both settled cables and the patch preview | Confirmed by source stacking rules; cable-specific visual reproduction remains to be completed |
| P1 | Jack targets are tiny, exact DOM hits; drag release uses a cached hover verdict | Confirmed by controller, jack markup, and state machine |
| P1 | Live dragging moves device boxes without updating the geometry used by cables or mounted children | Confirmed by separate preview and committed-layout paths |
| P1 | Cross-view portal stubs bypass the occlusion rules used for ordinary cables | Confirmed by rendering and geometry paths |
| P2 | Flip measurement and hidden-face estimates can produce stale or implausible cable positions | Confirmed by measurement scheduling and geometry; individual visual cases still need browser coverage |

## Investigation and reproduction

The simulator was open at `http://localhost:5173/` in Chrome. Browser automation was not exposed as a browser tab connection, so inspection used the native Chrome UI through computer use. Direct inspection of the Codex app was unavailable. No browser-console instrumentation or live worklet message trace was collected.

### Existing session: a valid click sequence ends without a cable

1. Computer and Scarlett 8i6 were both showing their rear panels.
2. Clicking the Computer USB socket displayed `Patching from Computer — click a destination jack`.
3. Clicking the Scarlett USB socket dismissed that banner, but no cable appeared.
4. Opening Debug showed `waiting for the previous patch change to be validated`. The health display also showed a running quantum count, so this was not simply an unstarted UI.

This establishes that the second click reached the commit path, and that an older request was still pending. It does **not** establish why that older acknowledgement was missing. The blocked request may predate this investigation.

### Fresh diagnostic session: rack covers mounted gear

Opened a separate tab to preserve the original scene, started its engine, added a Luma Leveler and an 8U rack, and dragged the Leveler into the rack. The device rendered in the rack and its toolbar changed to `eject from rack`, confirming that mounting succeeded.

Moving to/clicking an empty part of the rack revealed the rack toolbar and made the Leveler disappear behind the opaque cabinet. The accessibility tree still contained the Leveler and its eject control. This is an occlusion failure, not a missing device or unsuccessful mounting operation.

No scene Save was invoked. The diagnostic tab contains temporary, unsaved rack/device changes. The original tab was not reloaded; it retains the stalled validation state. Further browser work stopped when the user switched Chrome to another window. Successful fresh-session cable creation, drag-release behavior, and the full flip/portal matrix remain browser verification tasks.

### Existing automated checks

Ran from `web/`:

```sh
pnpm exec vitest run test/patching.test.ts test/cable-view.test.ts test/projection.test.ts test/placement.test.ts test/connections.test.ts
```

All **129 tests across 5 files passed**. These tests exercise pure transitions, geometry, legality, and projection. They do not exercise the actual DOM event adapter, CSS stacking, browser hit testing, or the asynchronous patch acceptance lifecycle. Passing them is consistent with the observed failures.

Existing unrelated working-tree edits, including Scarlett faceplate work, were left untouched. The findings below principally concern shared infrastructure.

## 1. Patch submission loses the gesture and can stay blocked indefinitely

Sources: [patch-controller.svelte.ts](../web/src/patch-controller.svelte.ts), `pointerUp` and `commitCable` (lines 104–133); [session.svelte.ts](../web/src/session.svelte.ts), `submitPatchCandidate`, `#patchAccepted`, and `#patchRejected` (lines 195–225); [App.svelte](../web/src/App.svelte), Debug status rendering around line 558.

The pure state machine returns a successful verdict with `state: null`. The controller calls `commitCable` and then assigns that null state. But `commitCable` ignores the boolean returned by `submitPatchCandidate`.

If the engine is unavailable, or an earlier request is pending, submission returns `false`. The held cable still disappears as if the operation completed. The user receives no visible explanation unless Debug is open. Both drag-and-drop and click + click converge on this path, so both can fail identically even with perfect socket targeting.

The session has one private `#pendingPatch` slot. It is cleared only by a matching `patchAccepted` or `patchRejected` message. There is no timeout/recovery state, and `load()`/`reload()` do not clear or reconcile it. A missing acknowledgement therefore blocks all later cable submissions for that session. An actual compiler rejection also discards the held cable and reports the explanation only through the general status field.

### Likely trigger to investigate, not yet proven

The acknowledgement protocol was introduced in `b7b8a21`. The worklet is loaded separately from `/processor.js`; that file is generated by [build-wasm.sh](../web/build-wasm.sh) and loaded once into an AudioWorklet context by [engine.ts](../web/src/engine.ts). A long-lived tab/HMR session can potentially retain an older worklet or callback wiring while the UI expects the new protocol.

Both the current [processor-impl.js](../web/worklet/processor-impl.js) and the generated `web/public/processor.js` on disk contain acceptance/rejection replies. Thus “the current source never sends an acknowledgement” would be incorrect. The version actually loaded in the failing tab was not inspected. Other possibilities include a lost reply, a dead processor, or a lifecycle mismatch between the session receiving messages and the session holding the pending request.

### Suggested fix

- Make submission return a structured result and let the controller retain the pick or show an explicit submitting state until submission/validation has a known outcome.
- Surface validation progress and rejection reasons beside the patch gesture, outside Debug. Preserve the selected endpoints for retry.
- Add an acknowledgement deadline and explicit recovery. A timeout means the result is unknown: reconcile or restart the engine from the authoritative scene before retrying, rather than blindly assuming the engine rejected the change.
- Include a protocol/build identifier in the ready handshake, and ensure worklet changes trigger a full session refresh during development.
- Log request ID, scene revision, send time, and reply time. Reproduce in a fresh tab to distinguish stale-session behavior from current-build behavior.

There is a related race: a candidate snapshots the **whole scene**, and `#patchAccepted` later replaces `this.scene` with that snapshot. Placement, facing, parameter, or other scene edits made while validation is pending can be overwritten. Other structural operations still call unacknowledged `hotSwap()` directly. Serialize topology mutations, associate acknowledgements with a scene revision, and merge only the validated change into current state or deliberately rebase it. Late replies after a load/restart must not resurrect an obsolete scene.

## 2. Hover stacking breaks both racks and cables

Sources: [WorldView.svelte](../web/src/widgets/WorldView.svelte), `showChrome`, `hideChromeSoon`, and item rendering (particularly line 264); [projection.ts](../web/src/projection.ts), `placedItemsFor`; [App.svelte](../web/src/App.svelte), `tipPatch` and `.rack-frame`.

The intended stack is:

| Element | Normal z-index |
| --- | --- |
| Rack frame | 0 |
| Rear-facing device | 1 |
| Ordinary cables | 2 |
| Front-facing device | 3 |
| Front-socket cable redraws, portal stubs, patch preview | 5 |

WorldView overrides **every item's entire stacking context** to z=6 when its hover toolbar is open, and z=10 while dragging. The hover state also lingers for 300 ms after pointer leave.

Consequences:

- A hovered rack's opaque background covers all its mounted devices. Devices are sibling world items, not descendants of the rack. The raised rack also intercepts pointer hits intended for their sockets and controls, linking rack rendering directly to failed patching.
- A hovered rear-facing device covers the z=2 cable that should reach its visible rear socket.
- A hovered front-facing device covers the z=5 cable redraw intended to keep a front connection visible.
- The held cable preview is also z=5, so the panel can hide it exactly when the pointer reaches a jack. Rendering still assumes the normal face-dependent layers; it does not account for the hover override.

### Suggested fix

Keep chassis/furniture stacking stable and render the hover controls in a separate overlay or portal. Raising only `.device-chrome` inside the current raised parent will not solve it: the parent is itself the stacking context. The rack body must remain behind mounted gear, and toolbar hit regions should cover only the controls.

Treat a rack and its mounted devices as a moving assembly for drag preview. Choose an explicit cable policy while dragging; merely changing the dragged panel's z-index repeats the visibility problem.

## 3. Exact socket hit testing and stale release decisions make gestures fragile

Sources: [Jack.svelte](../web/src/widgets/Jack.svelte); [device-drag.ts](../web/src/device-drag.ts), `DRAG_EXCLUDE`; [patch-controller.svelte.ts](../web/src/patch-controller.svelte.ts), pointer handlers; [patching.ts](../web/src/patching.ts), `pointerMove` and `pointerUp`.

Only the small `.connector` span carries `data-jack`. The controller requires either the event target or `document.elementFromPoint(...)` to resolve directly to that span. There is no nearest-jack search or screen-space tolerance. The “magnetic” behavior begins only **after** an exact hit. At scene zoom, a physically sized socket can be only a few screen pixels across. A near miss hits the panel and can start device dragging because that is excluded only for actual `[data-jack]` hits.

The click/drag threshold is 4 client pixels. A second click with more motion is classified as a pan and leaves the pick pending, regardless of whether it was actually a pan gesture. This can be especially confusing when the socket itself is similarly small.

Drag release does not hit-test the release point. It commits `state.verdict` from the last pointer move. A stale legal hover can commit after the pointer has left, or a stale miss can cancel after the release reaches the socket. Click + click does a fresh release hit test, so the two modes have different target semantics.

There is also no primary-button guard, active pointer ID, pointer capture, or `pointercancel` handler in the patch controller/window wiring. Window-level handlers alone do not provide a complete cancellation/ownership lifecycle. These are source-confirmed gaps; touch, multi-pointer, and outside-window failures were not reproduced here.

### Suggested fix

Use one shared target resolver for press, move, and release. Give sockets a usable screen-space hit area, exclude that entire area from device dragging, and optionally resolve the nearest visible, eligible jack within a bounded pixel radius. Do not select hidden-face jacks, off-surface duplicates, or jacks behind an opaque foreground device.

Re-evaluate target and connection legality on release for both modes. Track a primary pointer and distinguish patching, pending selection, panning, and device moving explicitly. Handle cancel/lost capture and pointer release outside the surface. Show the existing verdict reason (including “already connected” or connector mismatch), not just a red cable that vanishes.

The window-global hit resolver is also broader than the measurement scope: jack measurements are restricted to `measureRoot()`, but event hits are not. A duplicated physical faceplate in a focus window can resolve by the same device/port key and borrow the world socket's anchor. Either make those sockets explicitly non-patchable or provide a deliberate focus-window patching policy and coordinate mapping.

## 4. Cable and rack geometry lag behind live movement

Sources: [WorldView.svelte](../web/src/widgets/WorldView.svelte), local `drag`, `shown`, and `onMove`/`onEnd`; [App.svelte](../web/src/App.svelte), jack measurement effect at lines 120–136; [projection.ts](../web/src/projection.ts), `deviceRect`.

WorldView previews movement using private local `drag` state. App's cable layout and measured anchors are based on the committed scene. The scene is updated only on release. Consequently, during movement the panel can move while cable endpoints, clipping rectangles, and hidden-face estimates remain at its old position.

For racks, `shown()` offsets only the dragged rack item. Mounted device rectangles continue to derive from the rack's committed position, so the frame moves alone until release; its z=10 body can also cover the occupants. The placement math after release can be correct while the interactive presentation is broken.

### Suggested fix

Expose effective preview geometry to the renderer and cable layer. Apply the rack's preview transform to all mounted children, then derive endpoint positions and clips from that same geometry. Remeasure sockets after relevant layout changes, or transform stable local socket anchors along with their device. Avoid building a separate stale geometry model for cables.

## 5. Portal stubs do not respect socket-face occlusion

Sources: [App.svelte](../web/src/App.svelte), `onePortal` around line 620 and its overlay rendering around line 739; [cable-view.ts](../web/src/cable-view.ts), `cableAnchor`, `cableEndpoints`, and `clampEnd`.

Ordinary connections use `cableEndpoints`, including the hidden-face clamp, and draw in the z=2 layer with front-socket redraws where needed. A portal uses `cableAnchor` alone, then draws the **entire stub** in the z=5 overlay.

When the local socket is on the hidden face, `cableAnchor` returns an interior chassis estimate. Because the portal stub paints above the device, that estimate becomes a visible endpoint in the middle of the wrong face. The ordinary rear-facing hidden-end clamp is bypassed entirely. A connection can therefore change its apparent attachment behavior simply because the other device moves to a different wall/room.

### Suggested fix

Resolve the visible endpoint and the portal's free endpoint through the same face/occlusion geometry used by ordinary cables. Draw the physical stub with the appropriate masking/layers; put only the portal chip and its controls in the interaction overlay. Apply the same endpoint policy to held cables that survive a device flip or view switch.

## 6. Flip timing and hidden-end clipping are approximations

Sources: [jack-anchors.ts](../web/src/jack-anchors.ts); [Chassis.svelte](../web/src/widgets/Chassis.svelte); [App.svelte](../web/src/App.svelte), measurement effect; [cable-view.ts](../web/src/cable-view.ts), `resolveAnchor` and `segmentRectExit`; [connections.ts](../web/src/connections.ts), `cableControlPoints`.

Jack measurement runs on an animation frame and again after a hardcoded 480 ms; the flip lasts 450 ms. The first sample can capture an intermediate transform, then remain stale for most of the animation. Facing changes immediately in scene state, while measured socket positions still describe the old or partly rotated panel. The active patch preview takes raw source anchors without applying the scene's `faceAnchorable` policy.

Hidden sockets fall back to generic 38%/62% chassis-width estimates based on input/output direction. These are not the actual connector position or a routing side derived from that position. For a rear-facing panel with a hidden front socket, the code clamps the **straight segment** toward the other endpoint, but draws a sagging **cubic curve** afterward. The resulting cable can turn back into the chassis after the clamped endpoint. When the other endpoint is inside the same rectangle, the clamp returns the interior estimate unchanged.

### Suggested fix

Define the expected flip behavior first: animate cable anchors with the panel, or temporarily hide/non-interactively mark affected leads until the flip finishes. Use transition lifecycle events and actual layout invalidation rather than relying solely on a timeout. Store stable socket positions in device-local coordinates when practical.

For hidden-face leads, mask against the chassis silhouette or intersect the actual rendered curve, with an explicit routing choice around an edge. Test vertically separated and overlapping devices; moving the endpoint to a straight-line rectangle intersection alone does not guarantee correct curve visibility.

## Proposed delivery order and acceptance checks

1. **Restore trustworthy patch completion.** Cover successful acceptance, rejection, busy submission, missing acknowledgement, processor failure, load/restart, stale replies, and edits during validation. Verify both UI scene and engine topology. A failed submission must not silently discard the picked cable or block future work indefinitely.
2. **Separate hover controls from chassis stacking.** In a real browser, move the pointer across empty rack areas, rails, mounted devices, and toolbars. Occupants must remain visible and patchable. Visible front and rear socket leads must remain attached during hover and for the toolbar-close interval.
3. **Unify gesture targeting and lifecycle.** Test click + click and drag + drop at minimum, normal, and maximum zoom; near-edge hits; jitter; release without a fresh move; cancel; and cross-view pending picks. Include existing cable hit paths and focus windows.
4. **Share effective geometry during movement.** Move a connected device and a populated rack. Their visible geometry, cable anchors, clipping, and mounted children must follow throughout the gesture, not only after drop.
5. **Unify portal/ordinary cable occlusion and flip behavior.** Cover visible-front, hidden-front, visible-rear, and hidden-rear endpoints; both endpoint directions; all wall views; room changes; rack flips; and vertically separated/overlapping chassis. Top view intentionally omits cables and mounted device faceplates and should retain that behavior.

Keep the existing pure tests, but add focused browser integration coverage and session/transport tests. The current test seam supplies already-correct jack hits and a static layout, so it cannot detect the main failures observed here.
