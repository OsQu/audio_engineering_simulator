import { type DeviceDescriptor, descriptorFor } from "./catalog";
import { type PlacementPreview, resolveEffectivePlacement } from "./effective-placement";
import type { FlipTransition, PresentationState } from "./presentation-state";
import { effectiveFacing, type LayoutCtx, placedItemsFor } from "./projection";
import type { DeviceFacing, Scene } from "./scene-store";
import type { Rect2, Wall } from "./spatial";

export type FaceInstanceId = string;

export type PresentationStage =
  | { kind: "studio" }
  | { kind: "workbench"; role: "dut-front" | "dut-back" | "support" }
  | { kind: "focus"; windowId: string };

export interface ResolvedFace {
  readonly id: FaceInstanceId;
  readonly objectId: string;
  /** The settled side remains selected until the current transition finishes. */
  readonly face: DeviceFacing;
  readonly state: "presented" | "transitioning" | "hidden";
  readonly interaction: "enabled" | "disabled";
  readonly stage: PresentationStage;
  readonly transition?: FlipTransition;
}

interface ResolutionBase {
  readonly scene: Scene;
  readonly catalog: DeviceDescriptor[];
  readonly presentation: PresentationState;
}

export type FaceResolutionInput =
  | (ResolutionBase & {
      readonly kind: "studio";
      readonly space: string;
      readonly view: Wall | "top";
      readonly preview?: PlacementPreview;
    })
  | (ResolutionBase & {
      readonly kind: "workbench";
      readonly dutId: string;
      readonly preview?: PlacementPreview;
    })
  | (ResolutionBase & {
      readonly kind: "focus";
      readonly deviceId: string;
      readonly windowId: string;
      readonly face?: DeviceFacing;
    });

const keyPart = (value: string): string => encodeURIComponent(value);

export function faceInstanceId(stage: PresentationStage, objectId: string): FaceInstanceId {
  const object = keyPart(objectId);
  if (stage.kind === "studio") return `studio:${object}`;
  if (stage.kind === "focus") return `focus:${keyPart(stage.windowId)}:${object}`;
  return `workbench:${stage.role}:${object}`;
}

/** The object whose flip a face animates with: its rack when mounted, else the object itself. */
export function transitionOwner(scene: Scene, objectId: string): string {
  return scene.ui.placements[objectId]?.rack?.id ?? objectId;
}

/** A mounted device inherits its parent's transition exactly as it inherits its effective facing. */
export function transitionForObject(
  scene: Scene,
  presentation: PresentationState,
  objectId: string,
): FlipTransition | undefined {
  return presentation.flips[transitionOwner(scene, objectId)];
}

/** Current Story-10.4 policy: no physical lead is drawn while its object or parent rack flips. */
export function physicalLeadsSuppressed(
  scene: Scene,
  presentation: PresentationState,
  objectId: string,
): boolean {
  return transitionForObject(scene, presentation, objectId) !== undefined;
}

function resolvedSlot(
  stage: PresentationStage,
  objectId: string,
  settledFace: DeviceFacing,
  transition: FlipTransition | undefined,
  interaction: "enabled" | "disabled" = "enabled",
): ResolvedFace {
  return {
    id: faceInstanceId(stage, objectId),
    objectId,
    face: transition?.from ?? settledFace,
    state: transition ? "transitioning" : "presented",
    interaction: transition ? "disabled" : interaction,
    stage,
    ...(transition ? { transition } : {}),
  };
}

function resolveStudioFaces(
  input: Extract<FaceResolutionInput, { kind: "studio" }>,
): ResolvedFace[] {
  if (input.view === "top") return [];
  return input.scene.patch.devices.flatMap((device) => {
    const placement = resolveEffectivePlacement(
      input.scene,
      input.catalog,
      device.id,
      input.preview,
    );
    if (
      (placement.kind !== "studio-free" && placement.kind !== "studio-mounted") ||
      placement.space !== input.space ||
      placement.wall !== input.view
    ) {
      return [];
    }
    return [
      resolvedSlot(
        { kind: "studio" },
        device.id,
        placement.facing,
        transitionForObject(input.scene, input.presentation, device.id),
      ),
    ];
  });
}

function resolveWorkbenchFaces(
  input: Extract<FaceResolutionInput, { kind: "workbench" }>,
): ResolvedFace[] {
  return input.scene.patch.devices.flatMap((device) => {
    const placement = resolveEffectivePlacement(
      input.scene,
      input.catalog,
      device.id,
      input.preview,
    );
    if (placement.kind !== "workbench") return [];
    if (device.id === input.dutId) {
      return (["front", "back"] as const).map((face) => {
        const role = face === "front" ? "dut-front" : "dut-back";
        return resolvedSlot({ kind: "workbench", role }, device.id, face, undefined);
      });
    }
    return [
      resolvedSlot(
        { kind: "workbench", role: "support" },
        device.id,
        placement.facing,
        transitionForObject(input.scene, input.presentation, device.id),
      ),
    ];
  });
}

function resolveFocusFace(input: Extract<FaceResolutionInput, { kind: "focus" }>): ResolvedFace[] {
  const device = input.scene.patch.devices.find((candidate) => candidate.id === input.deviceId);
  if (!device || !descriptorFor(input.catalog, device.typeId)) return [];
  return [
    resolvedSlot(
      { kind: "focus", windowId: input.windowId },
      device.id,
      input.face ?? "front",
      undefined,
      "disabled",
    ),
  ];
}

/** Resolve presentation identity and face state without reading DOM, CSS, time, or camera state. */
export function resolveFaces(input: FaceResolutionInput): ResolvedFace[] {
  if (input.kind === "studio") return resolveStudioFaces(input);
  if (input.kind === "workbench") return resolveWorkbenchFaces(input);
  return resolveFocusFace(input);
}

// --- Resolved objects & physical paint order (Story 10.5) ---------------------------------------------
// Every rendered chassis (rack frame or device) gets a semantic layer and a position in one total order.
// Hover, keyboard focus, toolbar grace, and dragging are deliberately *not* inputs: interaction can never
// change physical occlusion. Controls live in a separate interaction overlay keyed by object ID.

/** Semantic physical layers, bottom → top. Renderers map each to a fixed CSS/SVG band. */
export type PaintLayer =
  | "furniture"
  | "rear-object"
  | "cable"
  | "front-object"
  | "cable-foreground"
  | "interaction";

/** The layers a physical object can occupy (cables and interaction chrome are not objects). */
export type ObjectLayer = Extract<PaintLayer, "furniture" | "rear-object" | "front-object">;

export const PAINT_LAYERS: readonly PaintLayer[] = [
  "furniture",
  "rear-object",
  "cable",
  "front-object",
  "cable-foreground",
  "interaction",
];

/** The renderer mapping both stages share: one fixed CSS z band per semantic layer. Nothing else
 *  (hover, focus, toolbar grace, drag) may set a chassis' z. */
export const LAYER_Z: Readonly<Record<PaintLayer, number>> = {
  furniture: 0,
  "rear-object": 1,
  cable: 2,
  "front-object": 3,
  "cable-foreground": 5,
  interaction: 8,
};

/** The outer chassis corner radius (mm), matching the renderer's `.content` clip. */
export const CHASSIS_RADIUS_MM = 6;

export interface ResolvedObject {
  readonly id: string;
  readonly kind: "rack" | "device";
  /** The effective rack this device is mounted in (including a candidate-mount preview), else null. */
  readonly parentId: string | null;
  readonly layer: ObjectLayer;
  /** Position in the total order `(layer, scene index, id)`; a later object covers an earlier one. */
  readonly order: number;
  /** World mm (y-up, `y` = bottom edge). Null where layout is not pure (the bench's DOM flow). */
  readonly rect: Rect2 | null;
}

/** An opaque chassis outline; Story 10.6 consumes these for cable mask sets. */
export interface Silhouette {
  readonly objectId: string;
  readonly rect: Rect2;
  readonly radius: number;
}

type UnorderedObject = Omit<ResolvedObject, "order"> & { readonly sceneIndex: number };

const layerRank = (layer: PaintLayer): number => PAINT_LAYERS.indexOf(layer);
const sideLayer = (side: DeviceFacing): ObjectLayer =>
  side === "back" ? "rear-object" : "front-object";

/** Total physical order: layer, then stable scene order, then ID. Never interaction or preview state. */
function ordered(objects: UnorderedObject[]): ResolvedObject[] {
  return [...objects]
    .sort(
      (a, b) =>
        layerRank(a.layer) - layerRank(b.layer) ||
        a.sceneIndex - b.sceneIndex ||
        (a.id < b.id ? -1 : a.id > b.id ? 1 : 0),
    )
    .map(({ sceneIndex: _sceneIndex, ...object }, order) => ({ ...object, order }));
}

export interface StudioObjectInput {
  readonly ctx: LayoutCtx;
  /** Resolved studio faces by object ID: a device's layer follows its *settled* side. */
  readonly faces: ReadonlyMap<string, ResolvedFace>;
  readonly preview?: PlacementPreview;
}

/** The studio's physical objects for the current view, already sorted by paint order. */
export function resolveStudioObjects(input: StudioObjectInput): ResolvedObject[] {
  const { ctx, faces, preview } = input;
  const rackIndex = new Map(ctx.scene.ui.racks.map((rack, index) => [rack.id, index]));
  const deviceIndex = new Map(ctx.scene.patch.devices.map((device, index) => [device.id, index]));
  return ordered(
    placedItemsFor(ctx, preview).flatMap((item): UnorderedObject[] => {
      const rackAt = rackIndex.get(item.id);
      if (rackAt !== undefined) {
        return [
          {
            id: item.id,
            kind: "rack",
            parentId: null,
            layer: "furniture",
            rect: item.rect,
            sceneIndex: rackAt,
          },
        ];
      }
      const deviceAt = deviceIndex.get(item.id);
      if (deviceAt === undefined) return [];
      const effective = resolveEffectivePlacement(ctx.scene, ctx.catalog, item.id, preview);
      if (effective.kind !== "studio-free" && effective.kind !== "studio-mounted") return [];
      // Top view presents no faces; free footprints sit above rack footprints.
      const side = ctx.view === "top" ? "front" : (faces.get(item.id)?.face ?? effective.facing);
      return [
        {
          id: item.id,
          kind: "device",
          parentId: effective.kind === "studio-mounted" ? effective.rack.id : null,
          layer: sideLayer(side),
          rect: item.rect,
          sceneIndex: deviceAt,
        },
      ];
    }),
  );
}

export interface BenchObjectInput {
  readonly scene: Scene;
  readonly dutId: string;
  /** Resolved workbench faces; a support device's layer follows its settled `support` slot side. */
  readonly faces: readonly ResolvedFace[];
}

/** The workbench's physical objects. The DUT presents both faces, so every one of its sockets is visible
 *  and it sits below the main cable pass (`rear-object`), as a back-shown device does; a front-shown
 *  support device covers the pass (its sockets are on the hidden back). */
export function resolveBenchObjects(input: BenchObjectInput): ResolvedObject[] {
  const support = new Map(
    input.faces
      .filter((face) => face.stage.kind === "workbench" && face.stage.role === "support")
      .map((face) => [face.objectId, face.face]),
  );
  return ordered(
    input.scene.patch.devices.map(
      (device, sceneIndex): UnorderedObject => ({
        id: device.id,
        kind: "device",
        parentId: null,
        layer:
          device.id === input.dutId
            ? "rear-object"
            : sideLayer(support.get(device.id) ?? effectiveFacing(input.scene, device.id)),
        rect: null,
        sceneIndex,
      }),
    ),
  );
}

/** The opaque outline of an object with a pure rect (racks included: their frames are opaque too). */
export function silhouetteOf(object: ResolvedObject): Silhouette | null {
  return object.rect ? { objectId: object.id, rect: object.rect, radius: CHASSIS_RADIUS_MM } : null;
}
