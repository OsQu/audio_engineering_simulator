// Pure view-policy resolvers shared by the studio and workbench roots. Geometry remains in projection.ts
// and cable-view.ts; this module supplies the small stage-specific answers those pure primitives need.

import type { CableResolutionInput, SurfacePoint, SurfaceSilhouette } from "./cable-resolution";
import type { SurfaceRect } from "./cable-view";
import { type PlacementPreview, resolveEffectivePlacement } from "./effective-placement";
import type { PresentationState } from "./presentation-state";
import { effectiveFacing, type LayoutCtx } from "./projection";
import type { DeviceFacing, Scene } from "./scene-store";
import type { ProjectedSlot, SocketGeometryIndex } from "./socket-geometry";
import type { Rect2 } from "./spatial";
import {
  CHASSIS_RADIUS_MM,
  physicalLeadsSuppressed,
  type ResolvedFace,
  type ResolvedObject,
  silhouetteOf,
} from "./view-resolution";

/** A world rect (mm, y-up, `y` = bottom edge) as a surface rect (y-down, top-left origin). */
export function worldRectToSurface(
  rect: Rect2,
  worldToSurface: (x: number, y: number) => SurfacePoint,
): SurfaceRect {
  const a = worldToSurface(rect.x, rect.y);
  const b = worldToSurface(rect.x + rect.width, rect.y + rect.height);
  return {
    x: Math.min(a.x, b.x),
    y: Math.min(a.y, b.y),
    width: Math.abs(b.x - a.x),
    height: Math.abs(b.y - a.y),
  };
}

export interface StudioCableInputArgs {
  readonly ctx: LayoutCtx;
  /** Resolved studio faces by object ID — exactly the devices presented in this view. */
  readonly faces: ReadonlyMap<string, ResolvedFace>;
  /** Resolved objects (preview-aware rects, total order). */
  readonly objects: readonly ResolvedObject[];
  readonly presentation: PresentationState;
  readonly geometry: SocketGeometryIndex;
  readonly preview?: PlacementPreview;
  readonly worldToSurface: (x: number, y: number) => SurfacePoint;
}

/** The studio's cable-resolution input. Every frame and silhouette comes from the preview-aware resolved
 *  objects, so leads follow drags; a face's presented side is its *settled* side (a mid-flip face is
 *  suppressed anyway). Local = presented in this view; top view presents no faces, so draws no leads. */
export function studioCableInput(args: StudioCableInputArgs): CableResolutionInput {
  const { ctx, faces, objects, presentation, geometry, preview, worldToSurface } = args;
  const silhouettes = new Map<string, SurfaceSilhouette>();
  for (const object of objects) {
    const outline = silhouetteOf(object);
    if (!outline) continue;
    silhouettes.set(object.id, {
      objectId: object.id,
      rect: worldRectToSurface(outline.rect, worldToSurface),
      radius: outline.radius,
      order: object.order,
      layer: object.layer,
    });
  }
  const slots = new Map<string, ProjectedSlot[]>();
  for (const face of faces.values()) {
    const frame = silhouettes.get(face.objectId)?.rect ?? null;
    slots.set(
      face.objectId,
      (["front", "back"] as const).map((side) => ({
        slot: { face: face.id, side },
        frame,
        presented: side === face.face,
      })),
    );
  }
  return {
    silhouettes,
    slots,
    geometry,
    membership: (id) => {
      if (faces.has(id)) return "local";
      const effective = resolveEffectivePlacement(ctx.scene, ctx.catalog, id, preview);
      return effective.kind === "studio-free" || effective.kind === "studio-mounted"
        ? "off-view"
        : "unresolved";
    },
    suppressed: (id) => physicalLeadsSuppressed(ctx.scene, presentation, id),
  };
}

/** Which faceplates the flat workbench renders. The DUT deliberately exposes both faces at once;
 * a supporting device follows its effective bench facing. */
export function benchVisibleFaces(scene: Scene, deviceId: string, dutId: string): DeviceFacing[] {
  return deviceId === dutId ? ["front", "back"] : [effectiveFacing(scene, deviceId)];
}

export interface BenchCableInputArgs {
  readonly scene: Scene;
  readonly dutId: string;
  /** Resolved bench objects (layer + total order). */
  readonly objects: readonly ResolvedObject[];
  /** Per object, its projected slot sides (frames = registered home + parent-resolved offset). */
  readonly slots: ReadonlyMap<string, readonly ProjectedSlot[]>;
  readonly presentation: PresentationState;
  readonly geometry: SocketGeometryIndex;
}

/** The workbench's cable-resolution input (U2): the same exits and masks as the studio. A supporting
 *  device's silhouette is its single flip-card frame, so a lead into its hidden side exits its nearest edge
 *  instead of attaching to a mirrored see-through socket. The DUT presents both faces (never an exit owner)
 *  and sits below the cable band (never above a foreground owner), so it contributes no silhouette.
 *  Everything on the bench is local. */
export function benchCableInput(args: BenchCableInputArgs): CableResolutionInput {
  const { scene, dutId, objects, slots, presentation, geometry } = args;
  const silhouettes = new Map<string, SurfaceSilhouette>();
  for (const object of objects) {
    if (object.id === dutId) continue;
    const frame = slots.get(object.id)?.find((slot) => slot.frame)?.frame;
    if (!frame) continue;
    silhouettes.set(object.id, {
      objectId: object.id,
      rect: frame,
      radius: CHASSIS_RADIUS_MM,
      order: object.order,
      layer: object.layer,
    });
  }
  const present = new Set(scene.patch.devices.map((device) => device.id));
  return {
    silhouettes,
    slots,
    geometry,
    membership: (id) => (present.has(id) ? "local" : "unresolved"),
    suppressed: (id) => physicalLeadsSuppressed(scene, presentation, id),
  };
}
