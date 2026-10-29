import { describe, expect, it } from "vitest";
import {
  beginFlip,
  cancelFlip,
  createPresentationState,
  finishFlip,
  unmountPresentation,
} from "../src/presentation-state";

describe("presentation transition state", () => {
  it("assigns monotonically increasing generations to starts and reversals", () => {
    const initial = createPresentationState();
    const first = beginFlip(initial, "device", "back");
    const reversal = beginFlip(first.state, "device", "front");

    expect(first.generation).toBe(1);
    expect(first.state.flips.device).toEqual({ generation: 1, from: "front", to: "back" });
    expect(reversal.generation).toBe(2);
    expect(reversal.state.flips.device).toEqual({ generation: 2, from: "back", to: "front" });
    expect(initial).toEqual({ flips: {}, generations: {} });
  });

  it("ignores stale finish and cancel events but settles the current generation", () => {
    const first = beginFlip(createPresentationState(), "device", "back");
    const reversal = beginFlip(first.state, "device", "front");

    expect(finishFlip(reversal.state, "device", first.generation)).toBe(reversal.state);
    expect(cancelFlip(reversal.state, "device", first.generation)).toBe(reversal.state);

    const settled = finishFlip(reversal.state, "device", reversal.generation);
    expect(settled.flips.device).toBeUndefined();
    expect(settled.generations.device).toBe(2);
  });

  it("invalidates unmounted handlers without allowing generation reuse", () => {
    const first = beginFlip(createPresentationState(), "device", "back");
    const unmounted = unmountPresentation(first.state, "device");
    const replacement = beginFlip(unmounted, "device", "back");

    expect(unmounted.flips.device).toBeUndefined();
    expect(unmounted.generations.device).toBe(2);
    expect(replacement.generation).toBe(3);
    expect(finishFlip(replacement.state, "device", first.generation)).toBe(replacement.state);
  });

  it("supports synchronous reduced-motion settlement through the same generation gate", () => {
    const started = beginFlip(createPresentationState(), "device", "back");
    expect(finishFlip(started.state, "device", started.generation)).toEqual({
      flips: {},
      generations: { device: 1 },
    });
  });

  it("tracks unrelated objects independently", () => {
    const a = beginFlip(createPresentationState(), "a", "back");
    const b = beginFlip(a.state, "b", "back");
    expect(a.generation).toBe(1);
    expect(b.generation).toBe(1);
  });
});
