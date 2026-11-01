import type { DeviceFacing } from "./scene-store";

/** One in-flight physical flip. The generation rejects late DOM lifecycle events. */
export interface FlipTransition {
  readonly generation: number;
  readonly from: DeviceFacing;
  readonly to: DeviceFacing;
}

/**
 * Pure presentation state. Generation counters survive settlement and unmount so a replacement
 * presentation of the same object can never reuse a token held by an obsolete DOM event handler.
 */
export interface PresentationState {
  readonly flips: Readonly<Record<string, FlipTransition>>;
  readonly generations: Readonly<Record<string, number>>;
}

export interface FlipStart {
  readonly state: PresentationState;
  readonly generation: number;
}

export function createPresentationState(): PresentationState {
  return { flips: {}, generations: {} };
}

const opposite = (face: DeviceFacing): DeviceFacing => (face === "front" ? "back" : "front");

/** Start a flip or reversal. Every call advances this object's generation. */
export function beginFlip(state: PresentationState, objectId: string, to: DeviceFacing): FlipStart {
  const generation = (state.generations[objectId] ?? 0) + 1;
  return {
    generation,
    state: {
      flips: {
        ...state.flips,
        [objectId]: { generation, from: opposite(to), to },
      },
      generations: { ...state.generations, [objectId]: generation },
    },
  };
}

/** Settle only the transition whose generation is still current. */
export function finishFlip(
  state: PresentationState,
  objectId: string,
  generation: number,
): PresentationState {
  if (state.flips[objectId]?.generation !== generation) return state;
  const { [objectId]: _settled, ...flips } = state.flips;
  return { ...state, flips };
}

/** A current cancellation settles against durable facing; an obsolete cancellation is ignored. */
export function cancelFlip(
  state: PresentationState,
  objectId: string,
  generation: number,
): PresentationState {
  return finishFlip(state, objectId, generation);
}

/**
 * Remove an object's active presentation and invalidate handlers retained by the unmounted DOM tree.
 * The counter intentionally remains as a tombstone for a later presentation of the same object ID.
 */
export function unmountPresentation(state: PresentationState, objectId: string): PresentationState {
  const invalidatedGeneration = (state.generations[objectId] ?? 0) + 1;
  const { [objectId]: _unmounted, ...flips } = state.flips;
  return {
    flips,
    generations: { ...state.generations, [objectId]: invalidatedGeneration },
  };
}
