// Story 10.7.2: the primary-pointer session reducer.
import { describe, expect, it } from "vitest";
import {
  IDLE,
  MOVE_THRESHOLD_PX,
  type SessionInput,
  type SessionState,
  step,
} from "../src/pointer-session";

const down = (patch: Partial<Extract<SessionInput, { kind: "down" }>> = {}): SessionInput => ({
  kind: "down",
  pointerId: 1,
  button: 0,
  isPrimary: true,
  pointerType: "mouse",
  at: { x: 100, y: 100 },
  ...patch,
});
const move = (x: number, pointerId = 1): SessionInput => ({
  kind: "move",
  pointerId,
  at: { x, y: 100 },
});
const up = (pointerId = 1): SessionInput => ({ kind: "up", pointerId, at: { x: 100, y: 100 } });

function run(inputs: SessionInput[]): { state: SessionState; effects: string[]; moved: boolean } {
  let state = IDLE;
  const effects: string[] = [];
  let moved = false;
  for (const input of inputs) {
    const next = step(state, input);
    state = next.state;
    effects.push(next.effect);
    moved = next.moved;
  }
  return { state, effects, moved };
}

describe("pointer session", () => {
  it("begins only for a primary pointer's primary button", () => {
    expect(step(IDLE, down()).effect).toBe("begin");
    expect(step(IDLE, down({ button: 2 })).effect).toBe("ignore");
    expect(step(IDLE, down({ isPrimary: false })).effect).toBe("ignore");
  });

  it("releases a click without movement (no final move needed)", () => {
    expect(run([down(), up()])).toEqual({
      state: IDLE,
      effects: ["begin", "release"],
      moved: false,
    });
  });

  it("turns a click into a drag past the threshold (4 px mouse, 8 px touch)", () => {
    expect(MOVE_THRESHOLD_PX).toMatchObject({ mouse: 4, pen: 4, touch: 8 });
    expect(run([down(), move(104), up()]).moved).toBe(false); // exactly 4 px: still a click
    expect(run([down(), move(104.5), up()]).moved).toBe(true);
    expect(run([down({ pointerType: "touch" }), move(107), up()]).moved).toBe(false);
    expect(run([down({ pointerType: "touch" }), move(109), up()]).moved).toBe(true);
  });

  it("stays moved once moved, even if the pointer returns", () => {
    expect(run([down(), move(120), move(100), up()]).moved).toBe(true);
  });

  it("ignores other pointers while active (multi-pointer)", () => {
    const r = run([down(), down({ pointerId: 2 }), move(150, 2), up(2), up()]);
    expect(r.effects).toEqual(["begin", "ignore", "ignore", "ignore", "release"]);
    expect(r.moved).toBe(false);
  });

  it("separates cancel from release", () => {
    expect(run([down(), { kind: "cancel", pointerId: 1 }]).effects).toEqual(["begin", "cancel"]);
    // Blur / unmount carry no pointer id.
    expect(run([down(), { kind: "cancel", pointerId: null }]).effects).toEqual(["begin", "cancel"]);
    // Another pointer's cancel doesn't end this session.
    expect(run([down(), { kind: "cancel", pointerId: 2 }, up()]).effects).toEqual([
      "begin",
      "ignore",
      "release",
    ]);
  });

  it("ignores a lost-capture cancel after a release (the gesture already committed)", () => {
    expect(run([down(), up(), { kind: "cancel", pointerId: 1 }]).effects).toEqual([
      "begin",
      "release",
      "ignore",
    ]);
  });
});
