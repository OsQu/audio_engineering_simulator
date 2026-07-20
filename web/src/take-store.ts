// OPFS take storage — the host's dumb byte store for the DAW's per-track WAV files (Story 5.11.6).
//
// The simulation owns the *format* (the worklet builds WAV headers via the sim's `wav_header`, and the
// engine's rings carry raw PCM frames); this module is only the host **byte storage** behind that seam.
// It runs in a dedicated Web Worker (storage-worker.ts) that owns OPFS **sync access handles** — the
// audio thread (the worklet) never touches disk, and the main thread relays bytes between the two.
//
// The `TakeStore` orchestration (which offset each write lands at, stripping the header on read) is kept
// free of any OPFS/wasm dependency and tested against an in-memory backend, since Node/Vitest has no
// OPFS. The real `FileSystemSyncAccessHandle` backend (opfs-backend.ts) is browser-only and verified
// in-browser.
//
// **One take per track (this cut).** Each track has at most one take file; re-recording overwrites it,
// playback plays every track's take from the top. A timeline of multiple positioned takes is future work.

/** The canonical WAV header length in bytes — where PCM frames begin. Mirrors the sim codec's
 *  `wav_header_len()` (44); a `header_len_matches` guard cross-checks it against the engine in-browser. */
export const HEADER_LEN = 44;

/**
 * The OPFS file name for a take, keyed by its owning **device** and track index — so two `computer`s
 * in one scene (each with its own track 0) never collide. Stable for a given `(deviceId, track)`, so
 * re-recording overwrites the same file. The device id is `encodeURIComponent`-escaped (it's a
 * UI-assigned string that may contain path-unsafe characters), which is injective, so distinct devices
 * always map to distinct names.
 */
export function takeFileName(deviceId: string, track: number): string {
  return `take-${encodeURIComponent(deviceId)}-${track}.wav`;
}

/**
 * A per-file random-access byte store — the subset of a `FileSystemSyncAccessHandle` the DAW needs,
 * abstracted so the record/playback logic is testable without OPFS. Implementations key files by name;
 * every operation is all-or-nothing on a whole file region.
 */
export interface TakeBackend {
  /** Ready `name` for a fresh take: create it if absent, truncate it to empty otherwise. */
  create(name: string): Promise<void>;
  /** Write `bytes` at absolute byte `offset` in `name`, growing the file as needed. */
  writeAt(name: string, offset: number, bytes: Uint8Array): Promise<void>;
  /** The whole current contents of `name`, or an empty array if it doesn't exist. */
  readAll(name: string): Promise<Uint8Array>;
  /** Delete `name`; a no-op if it doesn't exist. */
  remove(name: string): Promise<void>;
}

/**
 * An in-memory {@link TakeBackend} — the test/fallback backend (and the default when OPFS is
 * unavailable). Files are `Uint8Array`s in a map; `writeAt` grows the buffer as a real file would.
 */
export class MemoryBackend implements TakeBackend {
  #files = new Map<string, Uint8Array>();

  create(name: string): Promise<void> {
    this.#files.set(name, new Uint8Array(0));
    return Promise.resolve();
  }

  writeAt(name: string, offset: number, bytes: Uint8Array): Promise<void> {
    const cur = this.#files.get(name) ?? new Uint8Array(0);
    const end = offset + bytes.byteLength;
    if (end <= cur.byteLength) {
      cur.set(bytes, offset);
    } else {
      const grown = new Uint8Array(end);
      grown.set(cur, 0);
      grown.set(bytes, offset);
      this.#files.set(name, grown);
    }
    return Promise.resolve();
  }

  readAll(name: string): Promise<Uint8Array> {
    const f = this.#files.get(name);
    // Copy so a caller can't mutate the store's buffer through the returned view.
    return Promise.resolve(f ? f.slice() : new Uint8Array(0));
  }

  remove(name: string): Promise<void> {
    this.#files.delete(name);
    return Promise.resolve();
  }
}

/**
 * Records and loads per-track take files over a {@link TakeBackend}. It streams a take to disk without
 * ever holding it whole: a placeholder header first, then raw PCM appended block by block, then the
 * header overwritten with the true length at stop. The header bytes are produced by the caller (the
 * worklet, via the sim's WAV codec) — this class only decides *where* they land, so it needs no codec
 * and no wasm. Distinct tracks use distinct files, so recording one while reading another never
 * conflicts (per-file handles in the OPFS backend).
 */
export class TakeStore {
  #backend: TakeBackend;
  /** take file name → PCM bytes appended so far this take (excludes the header). */
  #recorded = new Map<string, number>();

  constructor(backend: TakeBackend) {
    this.#backend = backend;
  }

  /**
   * Begin a take on device `deviceId`'s track `track`: (re)create its file and write the placeholder
   * `header` (dataBytes = 0) at offset 0. `header` comes from the worklet's `wav_header(rate, ch, 0)`.
   */
  async beginRecord(deviceId: string, track: number, header: Uint8Array): Promise<void> {
    const name = takeFileName(deviceId, track);
    await this.#backend.create(name);
    await this.#backend.writeAt(name, 0, header);
    this.#recorded.set(name, 0);
  }

  /** Append raw PCM `pcm` after the header and any prior frames. No-op if the track isn't recording. */
  async appendRecord(deviceId: string, track: number, pcm: Uint8Array): Promise<void> {
    const name = takeFileName(deviceId, track);
    const written = this.#recorded.get(name);
    if (written === undefined) return; // not in a take — a stray drain after stop; drop it
    // Reserve the offset and bump the counter **synchronously**, before the `await` — so two appends
    // racing to the same track each get a distinct offset (the read-modify-write can't interleave).
    this.#recorded.set(name, written + pcm.byteLength);
    await this.#backend.writeAt(name, HEADER_LEN + written, pcm);
  }

  /**
   * Finish the take: overwrite offset 0 with `finalHeader` (the worklet's
   * `wav_header(rate, channels, dataBytes)` for the true length) and return the PCM byte count written.
   * A no-op returning 0 if the track wasn't recording.
   */
  async finishRecord(deviceId: string, track: number, finalHeader: Uint8Array): Promise<number> {
    const name = takeFileName(deviceId, track);
    const written = this.#recorded.get(name);
    if (written === undefined) return 0;
    await this.#backend.writeAt(name, 0, finalHeader);
    this.#recorded.delete(name);
    return written;
  }

  /**
   * The raw PCM of a track's take — the frames after the header — for playback framing and the
   * waveform view. Empty if the track has no take (or only a header). Never holds the file in the
   * engine; this is host memory, off the audio thread.
   */
  async loadPcm(deviceId: string, track: number): Promise<Uint8Array> {
    const all = await this.#backend.readAll(takeFileName(deviceId, track));
    // `slice` (not `subarray`) so the result owns a standalone buffer at offset 0 — clean to transfer
    // across the worker boundary and to view as a `Float32Array`.
    return all.byteLength > HEADER_LEN ? all.slice(HEADER_LEN) : new Uint8Array(0);
  }

  /** Whether the track has a stored take with at least one PCM frame. */
  async hasTake(deviceId: string, track: number): Promise<boolean> {
    const all = await this.#backend.readAll(takeFileName(deviceId, track));
    return all.byteLength > HEADER_LEN;
  }

  /** Delete the track's take file. */
  async removeTake(deviceId: string, track: number): Promise<void> {
    await this.#backend.remove(takeFileName(deviceId, track));
  }
}
