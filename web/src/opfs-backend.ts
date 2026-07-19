// The real OPFS byte backend for {@link TakeBackend} — browser-only, runs inside the storage worker.
//
// It keeps the DAW's take files in an OPFS `takes/` directory, one **sync access handle** per file. Sync
// access handles exist only in a Worker (never the window or the audio worklet), which is exactly why the
// storage layer is a dedicated worker: the audio thread never blocks on disk, and per-file handles let
// one take be read while another is written (the overdub case) without contention. Distinct tracks use
// distinct files, so their handles never collide; a single take file is only ever read *or* written at a
// time (a track is either recording or playing, never both), so one cached handle per file suffices.
//
// Not unit-tested (Node/Vitest has no OPFS); the storage logic is tested against `MemoryBackend`, and
// this backend is verified in-browser. It must still type-check and lint cleanly.

import type { TakeBackend } from "./take-store";

/** The subset of `FileSystemSyncAccessHandle` we use (its lib.dom typing varies across TS versions). */
interface SyncAccessHandle {
  read(buffer: ArrayBufferView, options?: { at?: number }): number;
  write(buffer: ArrayBufferView, options?: { at?: number }): number;
  truncate(size: number): void;
  getSize(): number;
  flush(): void;
  close(): void;
}

interface CreatableFileHandle {
  createSyncAccessHandle(): Promise<SyncAccessHandle>;
}

/** OPFS-backed {@link TakeBackend}: per-file sync access handles under an OPFS `takes/` directory. */
export class OpfsBackend implements TakeBackend {
  #dirName: string;
  #dir: FileSystemDirectoryHandle | null = null;
  /** name → its open sync access handle (exclusive lock; reused across a file's lifetime). */
  #handles = new Map<string, SyncAccessHandle>();

  constructor(dirName = "takes") {
    this.#dirName = dirName;
  }

  async #directory(): Promise<FileSystemDirectoryHandle> {
    if (!this.#dir) {
      const root = await navigator.storage.getDirectory();
      this.#dir = await root.getDirectoryHandle(this.#dirName, { create: true });
    }
    return this.#dir;
  }

  /** Open (and cache) the sync access handle for `name`, creating the file if needed. */
  async #handle(name: string): Promise<SyncAccessHandle> {
    const cached = this.#handles.get(name);
    if (cached) return cached;
    const dir = await this.#directory();
    const file = (await dir.getFileHandle(name, {
      create: true,
    })) as unknown as CreatableFileHandle;
    const handle = await file.createSyncAccessHandle();
    this.#handles.set(name, handle);
    return handle;
  }

  async create(name: string): Promise<void> {
    const handle = await this.#handle(name);
    handle.truncate(0);
    handle.flush();
  }

  async writeAt(name: string, offset: number, bytes: Uint8Array): Promise<void> {
    const handle = await this.#handle(name);
    handle.write(bytes, { at: offset });
    handle.flush();
  }

  async readAll(name: string): Promise<Uint8Array> {
    const handle = await this.#handle(name);
    const size = handle.getSize();
    const buf = new Uint8Array(size);
    if (size > 0) handle.read(buf, { at: 0 });
    return buf;
  }

  async remove(name: string): Promise<void> {
    const handle = this.#handles.get(name);
    if (handle) {
      handle.close();
      this.#handles.delete(name);
    }
    const dir = await this.#directory();
    await dir.removeEntry(name).catch(() => {}); // absent ⇒ nothing to remove
  }
}
