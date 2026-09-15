// Pure view-policy resolvers shared by the studio and workbench roots. Geometry remains in projection.ts
// and cable-view.ts; this module supplies the small stage-specific answers those pure primitives need.

import type { CableLayout } from "./cable-view";
import { resolveEffectivePlacement } from "./effective-placement";
import { deviceById, deviceRect, effectiveFacing, type LayoutCtx } from "./projection";
import type { DeviceFacing, Scene } from "./scene-store";

/** The studio's projection-backed cable policy: only the shown wall is measurable, and a device's
 * effective (rack-aware) face determines cable z behavior. */
export function sceneCableLayout(ctx: LayoutCtx): CableLayout {
  return {
    inView: (id) => {
      const effective = resolveEffectivePlacement(ctx.scene, ctx.catalog, id);
      return (
        (effective.kind === "studio-free" || effective.kind === "studio-mounted") &&
        effective.space === ctx.space &&
        effective.wall === ctx.wall
      );
    },
    faceAnchorable: (id, face) => face === effectiveFacing(ctx.scene, id, ctx.catalog),
    rect: (id) => {
      const device = deviceById(ctx.scene, id);
      return device ? deviceRect(ctx, id, device.typeId) : null;
    },
    clampsEstimate: (id) => effectiveFacing(ctx.scene, id, ctx.catalog) === "back",
    frontPatchOver: (id) => effectiveFacing(ctx.scene, id, ctx.catalog) === "front",
  };
}

/** Which faceplates the flat workbench renders. The DUT deliberately exposes both faces at once;
 * a supporting device follows its effective bench facing. */
export function benchVisibleFaces(scene: Scene, deviceId: string, dutId: string): DeviceFacing[] {
  return deviceId === dutId ? ["front", "back"] : [effectiveFacing(scene, deviceId)];
}

/** The workbench has no rooms or occluding spatial chassis: every measured jack is usable directly. */
export const benchCableLayout: CableLayout = {
  inView: () => true,
  faceAnchorable: () => true,
  rect: () => null,
  clampsEstimate: () => false,
  frontPatchOver: () => false,
};
