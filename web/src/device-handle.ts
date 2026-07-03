// The bridge between a device faceplate and the live engine, as a Svelte context value.
//
// A faceplate component (the generic `Panel`, or a bespoke per-device component) never wires the
// worklet itself: it composes the shared bound widgets (`Control`/`Socket`/`Reading`), which reference
// a param/port/readout **by id** and reach the engine through this handle. `Chassis` puts the handle in
// context; the wrappers read it back. So a device authors pure layout — the id → engine plumbing is
// resolved here, once, from the per-device closures `App` already builds (`valueFor`/`readingFor`/`onParam`).
//
// The ids are exactly the ones the Rust descriptor exposes (a param's exposed position, a port's id
// within its direction, a readout's position) — the layer seam holds: the catalog owns *what* exists;
// the faceplate owns *how* it's drawn and references it by id.

import { getContext, setContext } from "svelte";
import type { ParamDescriptor, PortDescriptor, PortDirection, ReadoutDescriptor } from "./catalog";

/** Everything a faceplate (and its bound widgets) needs to bind by id and drive the live engine. */
export interface DeviceHandle {
  /** Owning device instance id — tags jacks (`data-jack`) so the cable layer can locate them. */
  readonly device: string;
  /** Current value of the param at exposed id. */
  value(id: number): number;
  /** Apply a new value to the param at exposed id (resolves the descriptor and calls through to `onParam`). */
  set(id: number, value: number): void;
  /** Current live reading for a readout id (node→host lane); the engine floor for a missing/idle one. */
  reading(id: number): number;
  /** The descriptor for a param id, or `undefined` if the device has no such param. */
  param(id: number): ParamDescriptor | undefined;
  /** The descriptor for a port, addressed by direction + id, or `undefined` if absent. */
  port(dir: PortDirection, id: number): PortDescriptor | undefined;
  /** The descriptor for a readout id, or `undefined` if absent. */
  readout(id: number): ReadoutDescriptor | undefined;
}

/** Context key for the device handle — a Symbol so it can't collide with any string-keyed context. */
const DEVICE_HANDLE = Symbol("device-handle");

/** Publish the handle to descendants (called by `Chassis`, the faceplate's outer wrapper). */
export function setDeviceHandle(handle: DeviceHandle): void {
  setContext(DEVICE_HANDLE, handle);
}

/** Read the handle set by an ancestor `Chassis`. Bound widgets (`Control`/`Socket`/`Reading`) use this. */
export function getDeviceHandle(): DeviceHandle {
  const handle = getContext<DeviceHandle | undefined>(DEVICE_HANDLE);
  if (!handle) {
    throw new Error("no DeviceHandle in context — a bound widget must render inside a <Chassis>");
  }
  return handle;
}
