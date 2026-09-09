# 3D studio view spike

Status: agreed direction for an exploratory spike; implementation has not started.

## Purpose

Make the studio feel like a recognizable, inhabited space while preserving the existing audio
simulation and detailed equipment UI. Test whether a furnished, isometric-looking overview makes
finding and operating equipment more engaging without making patching cumbersome.

This extends the interaction direction in `PROJECT_PLAN.md` §7. The overview is another consumer of
the existing scene; the engine remains responsible for audio, and the Svelte equipment surfaces remain
responsible for close interaction.

## Agreed scope

- A stylized studio diorama: recognizable gear, furniture, acoustic panels, restrained materials,
  and soft lighting. The visual references supplied in the discussion were a furnished home-studio
  cutaway and a two-room control-room/live-room illustration. Aim for the latter's readable shapes
  with some of the former's material detail; photorealism is unnecessary.
- Two fixed views from opposite corners, each looking diagonally into the room. The camera positions
  are 180° apart around the room so the pair exposes all four walls. Allow pan/zoom; free orbit is
  outside the spike.
- Browse the existing equipment layout. Furniture and working-area metadata can be authored for the
  experiment; no user-facing placement editor is required.
- Click a **working area**—a desk, rack, or wall box—to enter a detailed operating view. Working areas
  replace the initially suggested floor-square interaction; a tile grid is not required.
- Patch within and between working areas, preserving an unfinished cable during navigation.
- Reuse existing Svelte equipment, controls, meters, focus surfaces, and cable interaction.
- No avatar, walking, reach restrictions, or movement simulation.

## Rendering and UI boundary

Use **Three.js with an orthographic camera** as the initial rendering choice. It draws the overview
only. Real geometry provides consistent opposite views, depth handling, and a natural representation
of gear on desks and in racks. Build the studio-specific adapter and navigation on top of the library;
do not introduce a full game engine or replace the Rust/WASM audio runtime.

The overview's simplified equipment models reference the same device instance IDs as the detailed UI.
They do not have separate audio state or duplicate control implementations.

The interaction layers are:

| Layer | Responsibility |
| --- | --- |
| 3D overview | Locate working areas and equipment; select destinations; show coarse activity |
| Svelte working-area view | Show the area's existing device panels and patch cables |
| Existing Svelte focus window | Provide a larger or device-specific operating surface |

Reuse the current panels, knobs, faders, sockets, meters, and focus components. Adapt their surrounding
layout and visibility selection from room/wall membership to working-area membership. Extract the
necessary composition from `App.svelte` rather than rebuilding device controls in 3D.

All layers must share **one authoritative scene and one running audio session**. Switching views must
not reconstruct the engine, interrupt playback/recording, reset controls, or discard an in-progress
patch. The current workbench route is not a navigation shortcut: it creates a separate session.

## Navigation and patching

Proposed cross-area workflow:

1. Select a desk, rack, or wall box in the overview.
2. In its Svelte view, select a socket and begin a cable.
3. Return to the overview while retaining the pending cable. A persistent strip identifies its source
   and offers cancellation.
4. Select the destination working area and complete the connection at a compatible socket.

Provide a working-area switcher in the detailed view as well, so repeated patching does not require
returning to the overview every time. Navigation must not implicitly complete or cancel a cable;
existing connection validation still applies.

Completed cables leaving the current working area should use labelled, clickable destination
indicators, for example “Wall box A · Input 3”. Build on the existing cable portal mechanism. Working
areas organize presentation and navigation, not which devices may connect to one another.

At overview scale, highlight a selected connection's destination or show a simplified route. Avoid
drawing every cable by default. Any overview route is illustrative: rendered distance must not silently
change electrical cable length or acoustic behavior.

## Camera, visibility, and activity

Hide the nearest two exterior walls in each corner view and show the farther pair. Check that switching
corners makes all working areas reachable. If tall furniture or partitions hide a selected area, trial
fading the obstruction or an explicit selection marker. Interior-partition behavior is exploratory;
the first fixture is one room.

Use a small amount of real session activity to test whether the studio feels alive: coarse meter
movement, a recording indicator, or an active computer screen. Derive these from existing readouts and
transport state. Keep updates lightweight and do not duplicate full detailed panels in the overview.

## Asset workflow

Use a small reusable 3D kit with consistent scale, origins, materials, and lighting. The two camera
views must render the same underlying objects, including credible backs and sides.

Start with:

- Room shell, floor, and acoustic wall panels.
- One desk and chair.
- One rack populated with existing equipment.
- Simplified computer/display, interface, keyboard, and speakers as needed by the fixture.
- One wall-box representation, subject to the functional scope question below.

Prefer scripted primitive geometry for boxes and repeated structures: desks, racks, chassis, speaker
cabinets, and acoustic panels. Use Blender for reusable modelling, material authoring, and export where
it helps; load exported models as glTF/GLB. Simple geometry can also be generated directly in Three.js.

AI assistance can supply art-direction references, texture ideas, modelling scripts, and candidate
models for irregular props such as chairs or guitars. Image-to-3D services are optional experiments,
not a required dependency. Inspect generated assets for dimensions, topology, materials, and cleanup
cost before accepting them.

Trial simplified front/rear artwork derived from the existing device designs as model textures. The
capture/export method still needs validation. Fine labels and functional controls remain in Svelte.

For each accepted asset record its source/provenance, dimensions, origin/pivot, orientation, selection
bounds, materials, and reproducible export settings. Provide a generic chassis fallback for device
types without custom artwork. Assemble assets together at actual overview scale early; judge consistency
and recognition in the room, not only in isolated asset previews.

Minimum additional tooling: Three.js and, for the proposed authored-asset workflow, Blender. An image
generator or paid 3D-generation service is optional. No installation or asset generation is part of
writing this plan.

## Existing foundations and gaps

| Existing code | Reuse and required adaptation |
| --- | --- |
| `web/src/scene-store.ts` | Spatial UI data is separate from the runnable patch. Add working-area and furniture presentation data here or alongside it. |
| `web/src/spatial.ts` | Reuse millimetres, XYZ coordinates, dimensions, and rack units. Resolve world transforms without storing separate positions per view. |
| `web/src/projection.ts` | Reuse rack membership and facing rules where applicable. Add a world-transform path for the overview rather than projecting everything to rectangles first. |
| `web/src/device-ui.ts` and existing widgets | Reuse device panels and focus surfaces without a control rewrite. |
| `web/src/session.svelte.ts` | Share scene state, readouts, and control paths across overview and working views. |
| `web/src/App.svelte` | Adapt/extract working-view composition and navigation; current visibility is room/wall based. |
| `web/src/cable-view.ts` and patch controller | Extend visibility and destination navigation to working areas; retain pending patch state across transitions. |
| `web/src/widgets/WorldView.svelte` and `web/src/world-api.ts` | Keep their useful detailed-view behavior. Their projected-rectangle/DOM coordinate contract is not a complete 3D scene interface. |

Specific spatial issues to resolve proportionately:

- **Orientation:** current placement ties orientation to a wall and retags the nearest wall when dragged.
  Derive compatible initial transforms, but define how physical orientation differs from working-view
  grouping. The spike does not need arbitrary user-authored rotations.
- **Rack occupants:** derive world positions from the rack and U-slot; do not use stale freestanding
  coordinates for mounted gear. Check frame-origin conventions and front/back facing.
- **Scale:** some device footprints were compacted for panel presentation, and rack depth is nominal.
  Audit the fixture's dimensions before treating all catalog boxes as faithful physical geometry.
- **Supports:** author desk/stand relationships so equipment sits on credible surfaces. A general
  furniture editor and support/collision system can wait.
- **Overlaps:** existing floor placement permits overlap. Identify problematic fixture placements;
  document any authored adjustments and keep both views on the same coordinate truth.
- **Multiple rooms:** rooms currently lack shared positions and adjacency. A connected studio floor
  plan, doors, and inter-room geometry are later extensions, not prerequisites for the one-room spike.
- **Persistence:** decide how new metadata is saved and how scenes without it fall back to existing
  room/wall views. Do not silently discard an existing layout merely to show the experiment.

## Work packages

These are high-level spike tasks, not approved task-level program designs.

1. **Fixture and interaction definition.** Choose one existing playable setup; identify its desk,
   populated rack, and wall-box working areas. Resolve the wall-box scope question. Specify which
   devices appear together and how navigation preserves a pending cable.
2. **Visual/asset proof.** Build the room, desk, and populated rack kit. Verify both opposite views,
   recognizable silhouettes, dimensions, mounting, and rear artwork. Establish a repeatable export
   path before producing more assets.
3. **Scene-backed overview.** Render actual scene device IDs and resolved placements; add pan/zoom,
   corner switching, working-area selection, fallback models, and basic occlusion handling.
4. **Shared Svelte interaction.** Open existing panels for the selected working area using the same
   session. Preserve focus and patching state as appropriate; add cross-area destination navigation.
5. **Activity and evaluation.** Add a few real activity indicators, run the end-to-end patching exercise,
   assess rendering/audio performance, and record the decision on continuing or revising the approach.

## Acceptance exercise

In one room containing a desk, populated rack, and wall box:

1. Start audio using an existing playable patch.
2. Inspect the studio from both corner views and select every working area, including small or partly
   occluded targets.
3. Patch wall box → rack → desk while changing working areas. If functional wall boxes are deferred,
   explicitly substitute existing source/destination devices and record that the wall-box part remains
   unvalidated.
4. Operate an EQ or another existing processor through its reused Svelte surface; hear/see the change.
5. Follow a completed cable's destination indicator and cancel another unfinished connection.
6. Return to the overview without interrupting the running session or losing state. Also verify
   recording continuity if recording is active in the fixture.

Check agreement between overview and detailed-view device identity/placement, correct rack-slot order,
and unchanged patch validation. Measure rendering responsiveness and audio health while navigating and
updating activity indicators on the target browser/machine; choose an explicit performance budget for
that target during implementation rather than claiming an unmeasured frame-rate guarantee.

The spike succeeds if equipment is recognizable, navigation and cross-area patching are comfortable,
existing controls remain intact, audio stays continuous, and adding another visual asset is repeatable.
Record asset creation/cleanup effort as well as runtime results. A convincing screenshot alone is not
sufficient validation.

## Open questions and boundaries

- **Wall boxes:** are these passive studio tie-lines connecting sockets in different places? If so,
  verify what the current patch model supports and scope their functional implementation separately
  from their visual representation. Decorative boxes cannot stand in for validated audio connectivity.
- **Working-view composition:** determine a sensible initial grouping and arrangement for desk/rack
  equipment, including front/rear access and patch cables. Reusing controls does not settle this layout.
- **Target environment:** agree on the browser, viewport, and representative device count used to judge
  performance and readability.

Outside this spike: avatars/reach gates, arbitrary camera orbit, placement editing, general collision
or pathfinding, room acoustics, geometric cable-length simulation, a full multi-room building editor,
and replacement of the audio engine or Svelte equipment UI.

The earlier operator-reach decision in `IMPLEMENTATION_PLAN.md` remains intact. This spike introduces
navigation between working areas, not physical restrictions on the operator.

## Research references

- [Three.js orthographic camera](https://threejs.org/docs/pages/OrthographicCamera.html)
- [Three.js picking](https://threejs.org/manual/en/picking.html)
- [PixiJS architecture](https://pixijs.com/8.x/guides/concepts/architecture) — alternative if the visual
  direction shifts to authored sprites; two-view asset consistency and depth ordering would need a trial.
- [Blender command-line rendering](https://docs.blender.org/manual/en/dev/advanced/command_line/render.html)
- [Meshy image-to-3D API](https://docs.meshy.ai/en/api/image-to-3d) — optional asset-workflow candidate,
  not a selected service or a quality guarantee.
