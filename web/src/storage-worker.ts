/// <reference lib="webworker" />
//
// The OPFS storage worker: owns the take files (via sync access handles, which need a Worker context)
// and services the main thread's record/load/remove commands. The audio worklet never touches disk;
// bytes reach here relayed through the main thread. Falls back to an in-memory store where OPFS is
// unavailable, so the app still runs (takes just don't persist across reloads).

import { OpfsBackend } from "./opfs-backend";
import type { StorageReply, StorageRequest } from "./storage-protocol";
import { MemoryBackend, type TakeBackend, TakeStore } from "./take-store";

const ctx = self as unknown as DedicatedWorkerGlobalScope;

// Feature-detect OPFS at runtime (absent in older browsers / some private modes); fall back to memory.
const hasOpfs =
  typeof navigator !== "undefined" && typeof navigator.storage?.getDirectory === "function";
const backend: TakeBackend = hasOpfs ? new OpfsBackend() : new MemoryBackend();
const store = new TakeStore(backend);

function reply(msg: StorageReply, transfer: Transferable[] = []): void {
  ctx.postMessage(msg, transfer);
}

ctx.onmessage = async (e: MessageEvent<StorageRequest>): Promise<void> => {
  const msg = e.data;
  try {
    switch (msg.cmd) {
      case "begin":
        await store.beginRecord(msg.deviceId, msg.track, msg.header);
        reply({ id: msg.id, ok: true });
        break;
      case "append":
        await store.appendRecord(msg.deviceId, msg.track, msg.pcm); // fire-and-forget
        break;
      case "finish": {
        const dataBytes = await store.finishRecord(msg.deviceId, msg.track, msg.header);
        reply({ id: msg.id, ok: true, dataBytes });
        break;
      }
      case "load": {
        const pcm = await store.loadPcm(msg.deviceId, msg.track);
        reply({ id: msg.id, ok: true, pcm }, [pcm.buffer]);
        break;
      }
      case "remove":
        await store.removeTake(msg.deviceId, msg.track);
        reply({ id: msg.id, ok: true });
        break;
    }
  } catch (err) {
    const id = "id" in msg ? msg.id : -1;
    reply({ id, ok: false, error: String((err as Error)?.message ?? err) });
  }
};
