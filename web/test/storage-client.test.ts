import { describe, expect, it } from "vitest";
import { StorageClient, type StorageWorker } from "../src/storage-client";
import type { StorageReply, StorageRequest } from "../src/storage-protocol";
import { MemoryBackend, TakeStore } from "../src/take-store";

// A fake storage worker that runs the real TakeStore over an in-memory backend — the same dispatch as
// storage-worker.ts, minus OPFS. Exercises client → protocol → store end to end in Node.
class FakeWorker implements StorageWorker {
  onmessage: ((e: MessageEvent<StorageReply>) => void) | null = null;
  #store = new TakeStore(new MemoryBackend());

  postMessage(msg: StorageRequest): void {
    void this.#handle(msg);
  }

  #reply(reply: StorageReply): void {
    this.onmessage?.({ data: reply } as MessageEvent<StorageReply>);
  }

  async #handle(msg: StorageRequest): Promise<void> {
    switch (msg.cmd) {
      case "begin":
        await this.#store.beginRecord(msg.deviceId, msg.track, msg.header);
        this.#reply({ id: msg.id, ok: true });
        break;
      case "append":
        await this.#store.appendRecord(msg.deviceId, msg.track, msg.pcm);
        break;
      case "finish": {
        const dataBytes = await this.#store.finishRecord(msg.deviceId, msg.track, msg.header);
        this.#reply({ id: msg.id, ok: true, dataBytes });
        break;
      }
      case "load": {
        const pcm = await this.#store.loadPcm(msg.deviceId, msg.track);
        this.#reply({ id: msg.id, ok: true, pcm });
        break;
      }
      case "remove":
        await this.#store.removeTake(msg.deviceId, msg.track);
        this.#reply({ id: msg.id, ok: true });
        break;
    }
  }
}

function header(dataBytes: number): Uint8Array {
  const h = new Uint8Array(44);
  new DataView(h.buffer).setUint32(40, dataBytes, true);
  return h;
}

function pcm(...values: number[]): Uint8Array {
  return new Uint8Array(new Float32Array(values).buffer.slice(0));
}

describe("storage-client", () => {
  it("records a take end to end and loads it back", async () => {
    const client = new StorageClient(new FakeWorker());

    await client.begin("pc", 0, header(0));
    client.append("pc", 0, pcm(0.1, 0.2));
    client.append("pc", 0, pcm(0.3));
    const dataBytes = await client.finish("pc", 0, header(12));

    expect(dataBytes).toBe(12); // 3 f32 frames
    const raw = await client.load("pc", 0);
    expect(
      Array.from(new Float32Array(raw.buffer.slice(raw.byteOffset, raw.byteOffset + 12))),
    ).toEqual([0.1, 0.2, 0.3].map((v) => Math.fround(v)));
  });

  it("keeps concurrent requests on different tracks independent", async () => {
    const client = new StorageClient(new FakeWorker());
    await Promise.all([client.begin("pc", 0, header(0)), client.begin("pc", 1, header(0))]);
    client.append("pc", 0, pcm(0.9));
    client.append("pc", 1, pcm(0.8));
    await Promise.all([client.finish("pc", 0, header(4)), client.finish("pc", 1, header(4))]);

    expect(Array.from(new Float32Array((await client.load("pc", 0)).buffer))).toEqual([
      Math.fround(0.9),
    ]);
    expect(Array.from(new Float32Array((await client.load("pc", 1)).buffer))).toEqual([
      Math.fround(0.8),
    ]);
  });

  it("load returns empty for a track with no take", async () => {
    const client = new StorageClient(new FakeWorker());
    expect((await client.load("pc", 0)).byteLength).toBe(0);
  });

  it("rejects when the worker replies with an error", async () => {
    // A worker that fails every correlated request.
    const failing: StorageWorker = {
      onmessage: null,
      postMessage(msg) {
        if ("id" in msg)
          this.onmessage?.({
            data: { id: msg.id, ok: false, error: "disk full" },
          } as MessageEvent<StorageReply>);
      },
    };
    const client = new StorageClient(failing);
    await expect(client.begin("pc", 0, header(0))).rejects.toThrow("disk full");
  });
});
