// The workbench's temp scene, round-tripped through the URL query so a Rust-rebuild → page-reload restores
// the bench exactly (rig, param overrides, the monitored tap) with zero scene management. Encoded as
// URL-safe base64 of the scene JSON — bench scenes are a handful of devices, well under any URL limit, so
// no compression is needed. Versioned like the localStorage store: a scene from an older schema is
// discarded (→ regenerate the default bench), never migrated (the temp scene is disposable by design).

import { SCHEMA_VERSION, type Scene } from "./scene-store";

// UTF-8 → URL-safe base64 (device labels carry non-Latin1 glyphs like × and →, so encode the bytes, not
// the string): base64, then +/ → -_ and strip = padding.
function toBase64Url(json: string): string {
  const bytes = new TextEncoder().encode(json);
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function fromBase64Url(param: string): string {
  const b64 = param.replace(/-/g, "+").replace(/_/g, "/");
  const bin = atob(b64);
  const bytes = Uint8Array.from(bin, (c) => c.charCodeAt(0));
  return new TextDecoder().decode(bytes);
}

/** Encode a scene for the URL query — URL-safe base64 of its JSON. */
export function encodeScene(scene: Scene): string {
  return toBase64Url(JSON.stringify(scene));
}

/** Decode a URL-query scene, or `null` when it's absent / malformed / from a different schema version —
 *  in which case the caller regenerates the default bench. Mirrors `scene-store.parseScene`'s guard. */
export function decodeScene(param: string | null | undefined): Scene | null {
  if (!param) return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(fromBase64Url(param));
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null) return null;
  const scene = parsed as Partial<Scene>;
  if (scene.schemaVersion !== SCHEMA_VERSION) return null;
  if (!scene.patch || !scene.ui) return null;
  return { schemaVersion: SCHEMA_VERSION, ui: scene.ui, patch: scene.patch };
}
