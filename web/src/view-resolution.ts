import { type DeviceDescriptor, descriptorFor } from "./catalog";
import { type PlacementPreview, resolveEffectivePlacement } from "./effective-placement";
import type { FlipTransition, PresentationState } from "./presentation-state";
import type { DeviceFacing, Scene } from "./scene-store";
import type { Wall } from "./spatial";

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
