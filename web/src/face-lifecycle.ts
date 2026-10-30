// The DOM adapter that reports when a flip's CSS transition settles. Pure presentation state
// (presentation-state.ts) decides *whether* a settle applies; this only binds one generation to the exact
// `CSSTransition` objects its style change started, via the Web Animations API. A bubbled
// `transitionend`/`transitioncancel` carries no generation, and a rapid reversal dispatches the superseded
// transition's cancel after the new one has begun — binding the promise in a closure makes that ordering
// irrelevant, because the stale promise reports its own (now obsolete) generation.

/** The chassis flip element (Chassis.svelte) and the face slot that names its transition owner. */
const FLIPPER = "[data-flipper]";
const ownerSelector = (owner: string): string => `[data-flip-owner="${CSS.escape(owner)}"]`;

/** Wait for the transitions started by `generation` under `owner`'s face slots, then report back.
 *
 * Call after the DOM reflects the new facing (after `tick()`). `getAnimations()` flushes style, so the
 * started transitions are visible synchronously. When none exist — reduced motion, zero duration, an
 * off-view object, or a rack with no visible occupants — the flip settles synchronously: there is
 * nothing left to wait for. Otherwise the generation settles when all of them have (both outcomes no-op
 * through the generation guard once superseded). */
export function awaitFlip(
  root: ParentNode,
  owner: string,
  generation: number,
  settle: (generation: number) => void,
  cancel: (generation: number) => void,
): void {
  const transitions = [...root.querySelectorAll<HTMLElement>(`${ownerSelector(owner)} ${FLIPPER}`)]
    .flatMap((el) => el.getAnimations())
    .filter((animation) => animation.playState !== "finished");
  if (transitions.length === 0) {
    settle(generation);
    return;
  }
  // A rack flip animates every visible occupant; the generation settles once all of them have, and
  // only cancels when none of them finished (every face was torn down or interrupted).
  void Promise.allSettled(transitions.map((animation) => animation.finished)).then((results) => {
    if (results.some((result) => result.status === "fulfilled")) settle(generation);
    else cancel(generation);
  });
}
