// DAW orchestration on the main thread (Story 5.11.6): the record relay and the playback feed loop that
// sit between the worklet (which owns the engine, rings, transport, and WAV codec) and the OPFS storage
// worker (which owns the take files). The worklet drains recorded PCM and reports the playhead; this
// routes those to storage and keeps each playing track's ring topped up ahead of the playhead.
//
// The one non-trivial algorithm — how much to feed each tick — is the pure `planPlaybackFeed`, unit
// tested. `DawController` is the thin stateful shell around it (browser integration: the worker + the
// worklet `send`), verified in-browser.

import type { ControlMessage } from "./engine";
import type { StorageClient } from "./storage-client";

/** Bytes per feed chunk (one f32 frame = 4 bytes). 4 KiB = 1024 mono frames. */
const FEED_CHUNK_BYTES = 4096;
/** Keep each playback ring filled to about here (half the engine's 32 KiB ring) — enough slack that a
 *  ~47×/s top-up never underruns, without racing far ahead of the playhead. */
const FEED_HIGHWATER_BYTES = 16384;

/** The result of planning one playback top-up: the byte ranges to feed, and the advanced cursor/fed. */
export interface FeedPlan {
  /** `[start, end)` byte ranges of the take's PCM to feed this tick, in order. */
  chunks: [number, number][];
  /** New read cursor (next unfed byte). */
  cursor: number;
  /** New total bytes fed into the ring since playback started. */
  fed: number;
}

/**
 * Plan how much of a take to feed into its playback ring this tick, keeping ring occupancy near the
 * high-water mark. `consumedBytes` is how much the engine has consumed since playback started
 * (`(playhead − playStart) × 4` for mono f32); ring occupancy is `fed − consumedBytes`. Feeds
 * `chunkBytes` at a time until occupancy reaches `highwaterBytes` or the take is exhausted. Pure.
 */
export function planPlaybackFeed(
  pcmLength: number,
  cursor: number,
  fed: number,
  consumedBytes: number,
  chunkBytes = FEED_CHUNK_BYTES,
  highwaterBytes = FEED_HIGHWATER_BYTES,
): FeedPlan {
  const chunks: [number, number][] = [];
  let c = cursor;
  let f = fed;
  while (c < pcmLength && f - consumedBytes < highwaterBytes) {
    const end = Math.min(c + chunkBytes, pcmLength);
    chunks.push([c, end]);
    f += end - c;
    c = end;
  }
  return { chunks, cursor: c, fed: f };
}

/** One track's live playback stream — its loaded PCM plus how far it's been fed. */
interface PlayStream {
  device: string;
  track: number;
  pcm: Uint8Array;
  cursor: number;
  fed: number;
  /** Playhead value when playback started, so occupancy tracks bytes consumed since then. */
  playStart: number;
}

/**
 * Routes recorded PCM to the storage worker and keeps playing tracks' rings fed. Constructed on engine
 * `ready` with the worklet `send` and the {@link StorageClient}. The record path is a straight relay
 * (the worklet brackets each take with started/stopped headers); playback is driven by the transport
 * ticks the worklet posts.
 */
export class DawController {
  #send: (msg: ControlMessage) => void;
  #storage: StorageClient;
  /** `${device}:${track}` → its live playback stream. */
  #playing = new Map<string, PlayStream>();

  constructor(send: (msg: ControlMessage) => void, storage: StorageClient) {
    this.#send = send;
    this.#storage = storage;
  }

  // --- Record relay: the worklet owns the lifecycle + headers; we just forward to storage. ----------

  /** A track began capturing — open its take file with the placeholder header. */
  recordStarted(device: string, track: number, header: Uint8Array): void {
    void this.#storage.begin(device, track, header);
  }

  /** A drained PCM chunk — append it to the take. */
  recorded(device: string, track: number, bytes: Uint8Array): void {
    this.#storage.append(device, track, bytes);
  }

  /** A track stopped capturing — finalize its take's header. Resolves once written. */
  async recordStopped(device: string, track: number, header: Uint8Array): Promise<void> {
    await this.#storage.finish(device, track, header);
  }

  // --- Playback: load each take once, then top up its ring each transport tick. ---------------------

  /** Begin playing `tracks` of `device` from the current `playhead`: load each take's PCM (skipping
   *  tracks with none) and register a stream. Idempotent-ish — replaces any existing streams. */
  async startPlayback(device: string, tracks: number[], playhead: number): Promise<void> {
    for (const track of tracks) {
      const pcm = await this.#storage.load(device, track);
      if (pcm.byteLength === 0) continue; // no take on this track
      this.#playing.set(`${device}:${track}`, {
        device,
        track,
        pcm,
        cursor: 0,
        fed: 0,
        playStart: playhead,
      });
    }
  }

  /** Stop all playback (transport stopped/seeked): drop every stream. The engine's rings drain to
   *  silence on their own. */
  stopPlayback(): void {
    this.#playing.clear();
  }

  /** Top up every playing stream of `device` for the current `playhead` — the transport-tick driver. */
  pump(device: string, playhead: number): void {
    for (const stream of this.#playing.values()) {
      if (stream.device !== device) continue;
      const consumed = Math.max(0, playhead - stream.playStart) * 4; // mono f32
      const plan = planPlaybackFeed(stream.pcm.byteLength, stream.cursor, stream.fed, consumed);
      for (const [start, end] of plan.chunks) {
        this.#send({
          type: "feedPlayback",
          device,
          track: stream.track,
          bytes: stream.pcm.slice(start, end),
        });
      }
      stream.cursor = plan.cursor;
      stream.fed = plan.fed;
      if (stream.cursor >= stream.pcm.byteLength) {
        this.#playing.delete(`${stream.device}:${stream.track}`); // fully fed; the ring plays it out
      }
    }
  }
}
