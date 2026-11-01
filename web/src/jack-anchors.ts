// The jack pointer-target gate. Socket *positions* are no longer swept from the DOM here: each face slot
// registers its own face-local geometry (face-geometry.ts → socket-geometry.ts, Story 10.4).

// The `data-jack` key a pointer target may patch, or null. A jack inside a face slot whose resolved
// interaction is disabled — a focus-window copy, or a face mid-flip — is not a physical patch target:
// a focus window renders the same keys as the stage, so without this gate it could start a cable at the
// stage socket's anchor.
export function patchableJackKey(target: Element | null): string | null {
  const jack = target?.closest<HTMLElement>("[data-jack]");
  if (!jack?.dataset.jack) return null;
  const slot = jack.closest<HTMLElement>("[data-face-instance]");
  return slot?.dataset.interaction === "disabled" ? null : jack.dataset.jack;
}
