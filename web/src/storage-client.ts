// Main-thread client for the OPFS storage worker (Story 5.11.6).
//
// Wraps the worker's message protocol as promises, correlating each request with its reply by a
// monotonic id. `append` is fire-and-forget (no reply — it happens per record block per armed track).
// The worker is abstracted as {@link StorageWorker} so the correlation logic is unit-testable with a
// fake; the session constructs the real `Worker(new URL("./storage-worker.ts", …), { type: "module" })`.

import type { StorageReply, StorageRequest } from "./storage-protocol";

/** The subset of `Worker` the client drives — postMessage out, a settable `onmessage` in. */
export interface StorageWorker {
  postMessage(message: StorageRequest, transfer?: Transferable[]): void;
  onmessage: ((e: MessageEvent<StorageReply>) => void) | null;
}

type Pending = { resolve: (r: StorageReply) => void; reject: (e: Error) => void };

/** Promise-based take storage over the worker: begin/append/finish a recording, load a take's PCM, or
 *  remove it. Distinct tracks use distinct files in the worker, so operations on different tracks are
 *  independent (the overdub case). */
export class StorageClient {
  #worker: StorageWorker;
  #nextId = 1;
  #pending = new Map<number, Pending>();

  constructor(worker: StorageWorker) {
    this.#worker = worker;
    worker.onmessage = (e) => this.#onReply(e.data);
  }

  #onReply(reply: StorageReply): void {
    const p = this.#pending.get(reply.id);
    if (!p) return; // a fire-and-forget failure (id -1) or a stale/unknown id — nothing to settle
    this.#pending.delete(reply.id);
    if (reply.ok) p.resolve(reply);
    else p.reject(new Error(reply.error));
  }

  #request(make: (id: number) => StorageRequest, transfer?: Transferable[]): Promise<StorageReply> {
    const id = this.#nextId++;
    return new Promise<StorageReply>((resolve, reject) => {
      this.#pending.set(id, { resolve, reject });
      this.#worker.postMessage(make(id), transfer);
    });
  }

  /** Open (or truncate) the track's take file and write the placeholder WAV `header`. */
  async begin(device: string, track: number, header: Uint8Array): Promise<void> {
    await this.#request(
      (id) => ({ id, cmd: "begin", deviceId: device, track, header }),
      [header.buffer],
    );
  }

  /** Append a chunk of raw PCM to the track's open take. Fire-and-forget (ordered with prior sends). */
  append(device: string, track: number, pcm: Uint8Array): void {
    this.#worker.postMessage({ cmd: "append", deviceId: device, track, pcm }, [pcm.buffer]);
  }

  /** Finalize the track's take: overwrite the header with `header`; resolves with the PCM byte count. */
  async finish(device: string, track: number, header: Uint8Array): Promise<number> {
    const r = await this.#request(
      (id) => ({ id, cmd: "finish", deviceId: device, track, header }),
      [header.buffer],
    );
    return r.ok ? (r.dataBytes ?? 0) : 0;
  }

  /** Load the track's take as raw PCM (header stripped), or an empty array if it has no take. */
  async load(device: string, track: number): Promise<Uint8Array> {
    const r = await this.#request((id) => ({ id, cmd: "load", deviceId: device, track }));
    return r.ok ? (r.pcm ?? new Uint8Array(0)) : new Uint8Array(0);
  }

  /** Delete the track's take file. */
  async remove(device: string, track: number): Promise<void> {
    await this.#request((id) => ({ id, cmd: "remove", deviceId: device, track }));
  }
}
