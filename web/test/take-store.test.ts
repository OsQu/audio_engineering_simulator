import { describe, expect, it } from "vitest";
import { HEADER_LEN, MemoryBackend, TakeStore, takeFileName } from "../src/take-store";

// A stand-in for the sim's WAV header (the real one is the worklet's wasm `wav_header`): a 44-byte
// buffer whose last 4 bytes carry the declared data length, so a test can tell the placeholder header
// (0) from the finalized one apart without depending on the real RIFF layout.
function fakeHeader(dataBytes: number): Uint8Array {
  const h = new Uint8Array(HEADER_LEN);
  new DataView(h.buffer).setUint32(HEADER_LEN - 4, dataBytes, true);
  return h;
}

function pcm(...values: number[]): Uint8Array {
  const buf = new Float32Array(values);
  return new Uint8Array(buf.buffer.slice(0));
}

function floats(raw: Uint8Array): number[] {
  return Array.from(
    new Float32Array(raw.buffer.slice(raw.byteOffset, raw.byteOffset + raw.byteLength)),
  );
}

const PC = "pc"; // a computer device id

describe("take-store", () => {
  it("names a take file by device + track, injectively (no cross-computer collision)", () => {
    expect(takeFileName(PC, 0)).toBe("take-pc-0.wav");
    expect(takeFileName(PC, 0)).toBe(takeFileName(PC, 0));
    // Two computers' track 0 must map to different files.
    expect(takeFileName("pc1", 0)).not.toBe(takeFileName("pc2", 0));
    // A path-unsafe device id is escaped, not passed through raw.
    expect(takeFileName("a/b:c", 0)).toBe("take-a%2Fb%3Ac-0.wav");
  });

  it("streams a take to disk header-first, then patches the header at stop", async () => {
    const backend = new MemoryBackend();
    const store = new TakeStore(backend);

    await store.beginRecord(PC, 0, fakeHeader(0));
    await store.appendRecord(PC, 0, pcm(0.1, 0.2));
    await store.appendRecord(PC, 0, pcm(0.3));
    const dataBytes = await store.finishRecord(PC, 0, fakeHeader(3 * 4));

    expect(dataBytes).toBe(12); // 3 f32 frames

    const file = await backend.readAll(takeFileName(PC, 0));
    expect(file.byteLength).toBe(HEADER_LEN + 12);
    // The header was overwritten with the true length (not the placeholder 0).
    expect(new DataView(file.buffer, file.byteOffset).getUint32(HEADER_LEN - 4, true)).toBe(12);
    // The PCM after the header round-trips exactly.
    const back = new Float32Array(file.buffer.slice(file.byteOffset + HEADER_LEN));
    expect(Array.from(back)).toEqual([0.1, 0.2, 0.3].map((v) => Math.fround(v)));
  });

  it("loads back just the raw PCM (header stripped)", async () => {
    const store = new TakeStore(new MemoryBackend());
    await store.beginRecord(PC, 0, fakeHeader(0));
    await store.appendRecord(PC, 0, pcm(0.5, -0.5));
    await store.finishRecord(PC, 0, fakeHeader(8));

    const raw = await store.loadPcm(PC, 0);
    expect(raw.byteLength).toBe(8);
    expect(floats(raw)).toEqual([0.5, -0.5]);
    expect(await store.hasTake(PC, 0)).toBe(true);
  });

  it("reports no take before recording and after removal", async () => {
    const store = new TakeStore(new MemoryBackend());
    expect(await store.hasTake(PC, 0)).toBe(false);
    expect((await store.loadPcm(PC, 0)).byteLength).toBe(0);

    await store.beginRecord(PC, 0, fakeHeader(0));
    await store.appendRecord(PC, 0, pcm(1.0));
    await store.finishRecord(PC, 0, fakeHeader(4));
    expect(await store.hasTake(PC, 0)).toBe(true);

    await store.removeTake(PC, 0);
    expect(await store.hasTake(PC, 0)).toBe(false);
  });

  it("a header-only take (nothing recorded) is not a take", async () => {
    const store = new TakeStore(new MemoryBackend());
    await store.beginRecord(PC, 0, fakeHeader(0));
    await store.finishRecord(PC, 0, fakeHeader(0));
    expect(await store.hasTake(PC, 0)).toBe(false);
    expect((await store.loadPcm(PC, 0)).byteLength).toBe(0);
  });

  it("records one track while reading another — the overdub case — with no cross-talk", async () => {
    const store = new TakeStore(new MemoryBackend());

    // Track 0 already has a finished take (the playing file).
    await store.beginRecord(PC, 0, fakeHeader(0));
    await store.appendRecord(PC, 0, pcm(0.1, 0.2, 0.3, 0.4));
    await store.finishRecord(PC, 0, fakeHeader(16));

    // Track 1 records while track 0's take is read back concurrently (interleaved).
    await store.beginRecord(PC, 1, fakeHeader(0));
    await store.appendRecord(PC, 1, pcm(0.9));
    const playing = await store.loadPcm(PC, 0); // read the playing file mid-record
    await store.appendRecord(PC, 1, pcm(0.8));
    await store.finishRecord(PC, 1, fakeHeader(8));

    // The playing file was unaffected by the concurrent record.
    expect(floats(playing)).toEqual([0.1, 0.2, 0.3, 0.4].map((v) => Math.fround(v)));
    // The recording file captured both appends, in order.
    expect(floats(await store.loadPcm(PC, 1))).toEqual([0.9, 0.8].map((v) => Math.fround(v)));
  });

  it("keeps two computers' track 0 as independent takes", async () => {
    const store = new TakeStore(new MemoryBackend());
    await store.beginRecord("pcA", 0, fakeHeader(0));
    await store.appendRecord("pcA", 0, pcm(0.11));
    await store.finishRecord("pcA", 0, fakeHeader(4));

    await store.beginRecord("pcB", 0, fakeHeader(0));
    await store.appendRecord("pcB", 0, pcm(0.22));
    await store.finishRecord("pcB", 0, fakeHeader(4));

    expect(floats(await store.loadPcm("pcA", 0))).toEqual([Math.fround(0.11)]);
    expect(floats(await store.loadPcm("pcB", 0))).toEqual([Math.fround(0.22)]);
  });

  it("ignores a stray append after stop (no active take)", async () => {
    const store = new TakeStore(new MemoryBackend());
    await store.beginRecord(PC, 0, fakeHeader(0));
    await store.appendRecord(PC, 0, pcm(0.5));
    await store.finishRecord(PC, 0, fakeHeader(4));
    // A drain that arrives after stop must not extend the finished file.
    await store.appendRecord(PC, 0, pcm(0.6));
    expect((await store.loadPcm(PC, 0)).byteLength).toBe(4);
  });
});
