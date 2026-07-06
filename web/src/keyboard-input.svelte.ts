// Shared computer-keyboard (QWERTY) note capture, bound to a session + a view-supplied target accessor —
// so the scene view and the workbench bench drive the identical glue instead of each re-wiring it. The
// **target selection** stays view-side (the 6.1 discipline: App derives it from focus; the bench points
// at its source/DUT); this owns only the reusable bit — the `playNote` wrapper that routes to the
// session's target-explicit `playNote`, and the attach/detach lifecycle (capture QWERTY only while a
// target is present). MIDI + the on-screen Keybed feed the same returned `playNote`.

import { wireKeyboard } from "./engine";
import { DEFAULT_VELOCITY } from "./notes";
import type { SceneSession } from "./session.svelte";

/** Wire QWERTY note capture to `session`, playing whatever device ids `targets()` returns (one, several
 *  for a "send to all", or none). Returns `playNote(on, note, velocity)` — fans the note out to each
 *  target via `session.playNote(…)` and is a no-op when there are none; feed it to an on-screen Keybed
 *  (and Web MIDI) too so every input source plays the same devices. Must be called during component init
 *  (it registers an `$effect`). QWERTY is captured only while there's at least one target — the effect
 *  re-attaches when the set changes and detaches when it empties, so operating a control never re-binds. */
export function wireKeyboardInput(
  session: SceneSession,
  targets: () => string[],
): (on: boolean, note: number, velocity?: number) => void {
  function playNote(on: boolean, note: number, velocity: number = DEFAULT_VELOCITY): void {
    for (const t of targets()) session.playNote(t, on, note, velocity);
  }
  $effect(() => {
    if (targets().length === 0) return;
    return wireKeyboard(playNote);
  });
  return playNote;
}
