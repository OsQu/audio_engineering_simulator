import { describe, expect, it } from "vitest";
import { planPlaybackFeed } from "../src/daw";

describe("planPlaybackFeed", () => {
  // Small chunk/highwater so the arithmetic is easy to read: feed 4-byte chunks up to 8 bytes ahead.
  const CHUNK = 4;
  const HIGH = 8;

  it("pre-fills up to the high-water mark at the start (nothing consumed yet)", () => {
    const plan = planPlaybackFeed(100, 0, 0, 0, CHUNK, HIGH);
    // occupancy = fed - 0 must stay < 8 → feed two 4-byte chunks (fed reaches 8, then stops).
    expect(plan.chunks).toEqual([
      [0, 4],
      [4, 8],
    ]);
    expect(plan.cursor).toBe(8);
    expect(plan.fed).toBe(8);
  });

  it("tops up only as the playhead consumes (steady state)", () => {
    // Already fed 8, consumed 4 → occupancy 4 < 8, room for one more chunk (fed→12, occ→8).
    const plan = planPlaybackFeed(100, 8, 8, 4, CHUNK, HIGH);
    expect(plan.chunks).toEqual([[8, 12]]);
    expect(plan.cursor).toBe(12);
    expect(plan.fed).toBe(12);
  });

  it("feeds nothing when the ring is already full to high-water", () => {
    const plan = planPlaybackFeed(100, 8, 8, 0, CHUNK, HIGH); // occupancy 8, not < 8
    expect(plan.chunks).toEqual([]);
    expect(plan.cursor).toBe(8);
    expect(plan.fed).toBe(8);
  });

  it("stops at the end of the take (last chunk is short)", () => {
    // 10-byte take, cursor 8, plenty of room → feed the final 2 bytes and no more.
    const plan = planPlaybackFeed(10, 8, 8, 8, CHUNK, HIGH);
    expect(plan.chunks).toEqual([[8, 10]]);
    expect(plan.cursor).toBe(10);
  });

  it("catches up after an underrun (consumed outran fed)", () => {
    // fed 8, consumed 16 → occupancy negative; feed until occupancy reaches high-water again.
    const plan = planPlaybackFeed(100, 8, 8, 16, CHUNK, HIGH);
    // Need fed - 16 < 8 to stop → fed must reach 24: from 8, that's four 4-byte chunks (8→24).
    expect(plan.chunks).toEqual([
      [8, 12],
      [12, 16],
      [16, 20],
      [20, 24],
    ]);
    expect(plan.fed).toBe(24);
  });

  it("is a no-op once the whole take has been fed", () => {
    const plan = planPlaybackFeed(10, 10, 10, 0, CHUNK, HIGH);
    expect(plan.chunks).toEqual([]);
    expect(plan.cursor).toBe(10);
  });
});
