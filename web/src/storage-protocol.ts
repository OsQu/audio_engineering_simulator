// The message contract between the main thread and the OPFS storage worker (Story 5.11.6).
//
// The main thread relays take bytes between the audio worklet (which owns the WAV codec + rings) and the
// worker (which owns OPFS). Every command but `append` is request/reply, correlated by `id`; `append` is
// fire-and-forget (it happens per record block per armed track, so a reply per append would be needless
// chatter — errors surface as an unsolicited failed reply with `id: -1`).

/** A command from the main thread to the storage worker. Takes are keyed by `(deviceId, track)` so
 *  multiple `computer`s in one scene never collide; `header`/`pcm` are raw bytes (transferable). */
export type StorageRequest =
  | { id: number; cmd: "begin"; deviceId: string; track: number; header: Uint8Array }
  | { cmd: "append"; deviceId: string; track: number; pcm: Uint8Array }
  | { id: number; cmd: "finish"; deviceId: string; track: number; header: Uint8Array }
  | { id: number; cmd: "load"; deviceId: string; track: number }
  | { id: number; cmd: "remove"; deviceId: string; track: number };

/** The worker's reply to a correlated request. `dataBytes` accompanies `finish`, `pcm` accompanies
 *  `load`; a failure carries the message (with `id: -1` for a failed fire-and-forget `append`). */
export type StorageReply =
  | { id: number; ok: true; dataBytes?: number; pcm?: Uint8Array }
  | { id: number; ok: false; error: string };
