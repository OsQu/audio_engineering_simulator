<script lang="ts">
  // The harness shell, now in Svelte 5. It owns the authoritative scene and the reactive UI state;
  // the engine/worklet bring-up and control transport live in engine.ts. Controls are rendered
  // **from the fetched device catalog** (not hardcoded ids) — a generic stepping stone; the
  // skeuomorphic panel widgets land in Story 4.2.3. Generic by device id throughout.

  import type { CableType, DeviceDescriptor, ParamDescriptor, PortDomain, PortKind } from "./catalog";
  import { descriptorFor, isPlayable } from "./catalog";
  import {
    type ControlMessage,
    healthSummary,
    type ReadyMessage,
    startEngine,
    wireKeyboard,
    wireMidi,
  } from "./engine";
  import { cablePathData, cableSpec, cableTypeIdFor, evaluateConnection } from "./connections";
  import type { ConnectVerdict, Endpoint } from "./connections";
  import type { Connection, Patch, PortRef } from "./scene";
  import { defaultScene, loadScene, newSpace, type Scene, saveScene, setSceneParam } from "./scene-store";
  import type { Rack } from "./scene-store";
  import {
    elevationToWorld,
    footprint,
    nearestFreeSlot,
    orientedSize,
    RACK_DEPTH_MM,
    RACK_UNIT_MM,
    RACK_WIDTH_MM,
    type Rect2,
    rectsOverlap,
    type Room,
    type Size3,
    type Vec3,
    type Wall,
    wallProjection,
  } from "./spatial";
  import Panel from "./widgets/Panel.svelte";
  import Screen from "./widgets/Screen.svelte";
  import Vu from "./widgets/Vu.svelte";
  import WorldView from "./widgets/WorldView.svelte";
  import type { WorldApi } from "./widgets/WorldView.svelte";

  let status = $state("idle");
  let health = $state("");
  let midiStatus = $state("MIDI: requesting access…");
  let started = $state(false);
  let ready = $state(false);
  let catalog = $state<DeviceDescriptor[]>([]);
  // Cable presets the picker offers for analog connections (fetched with the device catalog).
  let cables = $state<CableType[]>([]);
  let send = $state<((msg: ControlMessage) => void) | null>(null);
  // Master output peak (linear, ±1.0 = full scale), from the worklet's throttled level message.
  let level = $state(0);
  // Live device meter readings from the node→host lane, keyed by device id (values in readout-id
  // order). Updated ~47×/s from the worklet's `readouts` message.
  let readings = $state<Record<string, number[]>>({});
  // Static per-connection loading loss in dB (or null for digital/event connections), by connection
  // index (matching scene.patch.connections order). Seeded on `ready`, refreshed after each hot-swap.
  let losses = $state<(number | null)[]>([]);
  // A device's current reading for a readout id, or the meter floor if none has arrived yet.
  const readingFor = (device: string, id: number): number => readings[device]?.[id] ?? -120;
  // Format a reading: off-scale (near the floor) shows a dash.
  const fmtReading = (v: number): string => (v <= -55 ? "—" : v.toFixed(1));
  // The world layer's coordinate converters, bound out of WorldView so we can measure jack DOM positions
  // into surface space for precise cable anchoring. Undefined until the world surface mounts.
  let worldApi = $state<WorldApi | undefined>();

  // Monitor (listening) volume — a host-side output gain *outside* the simulation, persisted on its
  // own (a per-listener setting, not scene/simulation data). Defaults low so it doesn't blast.
  const VOLUME_KEY = "aes.volume";
  function loadVolume(): number {
    const s = localStorage.getItem(VOLUME_KEY);
    if (s === null) return 0.25;
    const raw = Number(s);
    return Number.isFinite(raw) ? Math.max(0, Math.min(1, raw)) : 0.25;
  }
  let volume = $state(loadVolume());
  let setVolume = $state<((gain: number) => void) | null>(null);

  function onVolume(v: number): void {
    volume = v;
    setVolume?.(v);
    localStorage.setItem(VOLUME_KEY, String(v));
  }

  // The page's authoritative scene: a saved one if present, else the default studio. The plain
  // `initialScene` const lets both `scene` and `currentSpace` seed from the same value without one
  // $state initializer reading another (which would only capture its initial value).
  const initialScene = loadScene() ?? defaultScene();
  let scene = $state<Scene>(initialScene);

  // Live control-param values, keyed `device:paramId`, mirrored into the scene on change so they
  // persist on save. Re-seeded from the scene whenever it's (re)loaded.
  let paramValues = $state<Record<string, number>>({});

  // The playable instrument (first device whose descriptor has an event input) drives the keyboard.
  const synthDevice = $derived(
    scene.patch.devices.find((d) => {
      const desc = descriptorFor(catalog, d.typeId);
      return desc ? isPlayable(desc) : false;
    }),
  );
  const key = (device: string, paramId: number): string => `${device}:${paramId}`;

  // The current value of a device-local param: the live override if any, else the descriptor default.
  function paramValue(deviceId: string, desc: DeviceDescriptor, id: number): number {
    const v = paramValues[key(deviceId, id)];
    return v !== undefined ? v : (desc.params.find((p) => p.id === id)?.default ?? 0);
  }

  // A plain (non-proxied) deep copy of the patch for crossing to the worklet: `$state` wraps the
  // scene in a reactive Proxy, which `postMessage` cannot structured-clone (DataCloneError).
  const plainPatch = (): Patch => $state.snapshot(scene.patch);

  // --- The spatial world ---------------------------------------------------------------------------
  // The space (room) currently shown; switching changes the rendered/interactable set.
  let currentSpace = $state(initialScene.ui.spaces[0]?.id ?? "");
  // The view within the current space: one of the four wall elevations, or the top-down floor plan.
  // "top" lands in Story 4.6.4; the wall elevations render here.
  let currentView = $state<Wall | "top">("front");
  // The wall currently in elevation, or null in the top view.
  const currentWall = $derived<Wall | null>(currentView === "top" ? null : currentView);
  // The current space's room (the four walls derive from this rectangle). Falls back to a default box
  // if the space id can't be resolved (shouldn't happen — every space carries a room since schema 7).
  const room = $derived<Room>(
    scene.ui.spaces.find((s) => s.id === currentSpace)?.room ?? {
      width: 4000,
      depth: 3000,
      height: 1400,
    },
  );

  const FRAME_MARGIN = 14; // mm of rack frame drawn around the U-slot region
  const GRID_MM = 50; // free-placement snap grid (world mm) — eases aligning gear on the floor
  // Human labels for the wall-view switcher.
  const WALL_LABELS: Record<Wall, string> = { front: "Front", back: "Back", left: "Left", right: "Right" };

  const deviceById = (id: string) => scene.patch.devices.find((d) => d.id === id);
  const rackById = (id: string) => scene.ui.racks.find((r) => r.id === id);
  const isRack = (id: string) => rackById(id) !== undefined;

  // How many U a device occupies — 0 if it isn't rackmount gear (so it never mounts in a rack).
  function deviceUnits(typeId: string): number {
    const desc = descriptorFor(catalog, typeId);
    return desc && desc.formFactor.kind === "rackmount" ? desc.formFactor.rackUnits : 0;
  }

  // A rack's frame footprint (world mm) — the U-slot column plus the drawn margin, RACK_DEPTH deep.
  const rackFrameSize = (rack: Rack): Size3 => ({
    width: RACK_WIDTH_MM + 2 * FRAME_MARGIN,
    height: rack.slots * RACK_UNIT_MM + 2 * FRAME_MARGIN,
    depth: RACK_DEPTH_MM,
  });

  // A rack's frame rect in the current wall's elevation (the draggable box drawn behind its gear).
  function rackRect(rack: Rack): Rect2 {
    return wallProjection(rack.position, orientedSize(rackFrameSize(rack), rack.wall), rack.wall, room);
  }

  // A device's rect in the **current wall's elevation** — derived from its rack + U-slot when mounted,
  // else from its free-standing position projected onto its wall. Its elevation *width* is always the
  // panel width (a unit faces into the room on every wall); only the horizontal *position* is wall-
  // dependent, so mounted gear is placed inside the already-projected rack frame. `null` without a
  // descriptor/placement.
  function deviceRect(deviceId: string, typeId: string): Rect2 | null {
    const desc = descriptorFor(catalog, typeId);
    const place = scene.ui.placements[deviceId];
    if (!desc || !place) return null;
    const size = footprint(desc.formFactor);
    if (place.rack) {
      const rack = rackById(place.rack.id);
      if (!rack) return null;
      const frame = rackRect(rack);
      return {
        x: frame.x + FRAME_MARGIN,
        y: frame.y + FRAME_MARGIN + place.rack.uSlot * RACK_UNIT_MM,
        width: RACK_WIDTH_MM,
        height: size.height,
      };
    }
    return wallProjection(place.position, orientedSize(size, place.wall), place.wall, room);
  }

  // The items to render in the current space: rack frames first (behind), then devices (on top).
  // `z` interleaves items with the single cable layer (z 2): racks at the bottom, a device showing its
  // back *below* the cables (they plug into its visible sockets), one facing front *above* them (cables
  // tuck behind its panel). This is what makes a continuous cable occlude correctly per device.
  const placedItems = $derived([
    ...scene.ui.racks
      .filter((r) => r.space === currentSpace && r.wall === currentWall)
      .map((r) => ({ id: r.id, rect: rackRect(r), background: true, z: 0 })),
    ...scene.patch.devices
      .filter((d) => {
        const p = scene.ui.placements[d.id];
        return p?.space === currentSpace && p.wall === currentWall;
      })
      .map((d) => ({
        id: d.id,
        rect: deviceRect(d.id, d.typeId),
        z: scene.ui.placements[d.id]?.facing === "back" ? 1 : 3,
      }))
      .filter((it): it is { id: string; rect: Rect2; z: number } => it.rect !== null),
  ]);

  // --- Patch cables --------------------------------------------------------------------------------
  // A stable key for a connection (its two endpoints), for the {#each} in the cable overlay.
  const connKey = (c: Connection): string =>
    `${c.from.device}:${c.from.port}->${c.to.device}:${c.to.port}`;

  // Measured jack-connector centres in surface space, keyed "device:direction:port" (from each jack's
  // `data-jack` attribute). Populated by measureJacks after layout; lets a cable anchor at the real
  // socket when a device's back is shown, falling back to the chassis edge otherwise.
  let jackAnchors = $state<Record<string, { x: number; y: number }>>({});

  const jackKey = (device: string, direction: "input" | "output", port: number): string =>
    `${device}:${direction}:${port}`;

  // Measure every rendered jack's centre into surface-local space (pan/zoom-invariant, because
  // clientToSurface divides out the transform). Cheap — a handful of getBoundingClientRect — and only
  // re-run on layout changes (below), not per frame.
  function measureJacks(): void {
    if (!worldApi) return;
    const next: Record<string, { x: number; y: number }> = {};
    for (const el of document.querySelectorAll<HTMLElement>("[data-jack]")) {
      const key = el.dataset.jack;
      if (!key) continue;
      const r = el.getBoundingClientRect();
      if (r.width === 0 && r.height === 0) continue; // not laid out / hidden
      next[key] = worldApi.clientToSurface(r.left + r.width / 2, r.top + r.height / 2);
    }
    jackAnchors = next;
  }

  // Re-measure when layout-affecting state changes (placement / flip / space / catalog / api ready).
  // Pan/zoom needn't trigger it — surface-local coords are invariant. Measure after paint, and again
  // once the 0.45s flip transition has settled (so a just-flipped back reports its final jack positions).
  $effect(() => {
    void ready;
    void currentSpace;
    void currentView;
    void catalog.length;
    void worldApi;
    JSON.stringify(scene.ui.placements);
    JSON.stringify(scene.patch.connections);
    const raf = requestAnimationFrame(measureJacks);
    const settle = setTimeout(measureJacks, 480);
    return () => {
      cancelAnimationFrame(raf);
      clearTimeout(settle);
    };
  });

  // The surface-local anchor for one end of a cable. When the device's **back** is shown, anchor at the
  // measured socket centre; otherwise (front-facing, or not yet measured) fall back to the chassis edge
  // (output → right, input → left, spread by port index). `null` when the device isn't in the shown
  // space — a cross-space end is drawn as a portal (Story 4.4.6), not a continuous cable.
  function cableAnchor(
    ref: PortRef,
    direction: "input" | "output",
    api: WorldApi,
  ): { x: number; y: number } | null {
    const device = deviceById(ref.device);
    const place = scene.ui.placements[ref.device];
    if (!device || !place || !inView(ref.device)) return null;
    if (place.facing === "back") {
      const jack = jackAnchors[jackKey(ref.device, direction, ref.port)];
      if (jack) return jack; // precise: the real socket on the shown back panel
    }
    const rect = deviceRect(ref.device, device.typeId);
    if (!rect) return null;
    // Front-facing (or not yet measured): the sockets sit centred on the back panel, so estimate near
    // the chassis centre — nudged toward the signal-flow direction (output right, input left) so the
    // cable emerges toward its neighbour. This end is drawn *behind* the device (hidden), so a rough
    // estimate is enough; it just needs to look plausible where it tucks under the edges.
    const wx = rect.x + rect.width * (direction === "output" ? 0.62 : 0.38);
    const wy = rect.y + rect.height * 0.45;
    return api.worldToSurface(wx, wy);
  }

  // Cable occlusion is handled by z-order, not by the cable: a single continuous lead is drawn in the
  // cable layer (z 2), and each device sits above or below it by facing (see placedItems' `z`) — a
  // back-facing device (visible sockets) below, a front-facing one above. So a cable plugs into a visible
  // back socket yet tucks behind a front panel, with no split. (Sockets are back-mounted today.)

  // Is a device visible in the current view — i.e. in the shown space *and* against the shown wall? Only
  // one wall of one space renders at a time, so this is the "same view" test the cable renderer keys on.
  const inView = (deviceId: string): boolean => {
    const p = scene.ui.placements[deviceId];
    return p?.space === currentSpace && p.wall === currentWall;
  };
  // A cable with both ends in view draws as a full lead; exactly one end here → a portal stub toward the
  // other view (the 4.4 mechanism, generalized from "other room" to "other wall/room"); neither end here
  // → not shown. The engine sees a plain mono connection either way — walls/spaces/portals are UI-only.
  const bothInView = (c: Connection): boolean => inView(c.from.device) && inView(c.to.device);
  const oneInView = (c: Connection): boolean => inView(c.from.device) !== inView(c.to.device);
  const spaceName = (id: string): string => scene.ui.spaces.find((s) => s.id === id)?.name ?? id;
  // A short label for where a cable's off-view end lives: the room name if it's in another space, else
  // the wall name (a different wall of this same room).
  function otherEndLabel(deviceId: string): string {
    const p = scene.ui.placements[deviceId];
    if (!p) return "?";
    return p.space !== currentSpace ? spaceName(p.space) : WALL_LABELS[p.wall];
  }
  // How far a portal stub extends from its jack, in surface mm.
  const PORTAL_LEN = 180;

  // --- Patching (drag or click-to-pick) -------------------------------------------------------------
  // An in-progress patch from a source jack: the source end + its anchor, the moving free end (surface
  // coords), and — when hovering a candidate jack — the verdict so we can colour the cable + commit.
  // `mode` is "drag" while the pointer is held (same-view patching), or "pending" after a *click* — a
  // held cable that survives a wall/room switch, so you can complete a **cross-view** patch by clicking
  // the source jack, turning to the other wall/room, and clicking the destination jack.
  let dragCable = $state<{
    source: Endpoint;
    srcPoint: { x: number; y: number };
    free: { x: number; y: number };
    over: boolean;
    legal: boolean;
    verdict: ConnectVerdict | null;
    mode: "drag" | "pending";
  } | null>(null);
  // Pointer-down bookkeeping to tell a click (→ pending pick) from a drag (moved past a small threshold).
  let cableDown = { x: 0, y: 0, moved: false };

  // The display name of the pending patch's source device (for the "patching from…" banner).
  const pendingSourceName = $derived.by((): string | null => {
    if (dragCable?.mode !== "pending") return null;
    const dev = deviceById(dragCable.source.device);
    return (dev && descriptorFor(catalog, dev.typeId)?.name) || dragCable.source.device;
  });

  const jackKeyOf = (e: Endpoint): string => jackKey(e.device, e.direction, e.port);

  // Resolve a `data-jack` value ("device:direction:port") to a full Endpoint (with the port's domain
  // from the descriptor), or null if it doesn't name a real port.
  function endpointFromJackKey(key: string): Endpoint | null {
    const [device, direction, portStr] = key.split(":");
    if (!device || (direction !== "input" && direction !== "output")) return null;
    const port = Number(portStr);
    const dev = deviceById(device);
    const desc = dev ? descriptorFor(catalog, dev.typeId) : undefined;
    const pd = desc?.ports.find((p) => p.direction === direction && p.id === port);
    if (!pd) return null;
    return { device, port, direction, domain: pd.domain };
  }

  // Pointer-down on a jack connector. Normally starts a drag (only reachable on a shown back panel; a
  // front-facing device's back is rotated away and non-interactive). While a **pending** cable is held,
  // this is the *second* click: complete onto a legal destination jack, cancel by re-clicking the source
  // jack, and otherwise stay pending (so panning / rearranging / an illegal jack don't lose the patch).
  function onCablePointerDown(e: PointerEvent): void {
    if (!worldApi) return;
    const el = (e.target as HTMLElement | null)?.closest<HTMLElement>("[data-jack]");
    const key = el?.dataset.jack;

    if (dragCable?.mode === "pending") {
      if (!key) return; // empty space — keep the pending pick
      if (key === jackKeyOf(dragCable.source)) {
        dragCable = null; // re-clicking the source cancels
        return;
      }
      const target = endpointFromJackKey(key);
      if (target) {
        const verdict = evaluateConnection(dragCable.source, target, scene.patch.connections);
        if (verdict.ok) {
          commitCable(verdict);
          dragCable = null;
        }
      }
      return; // illegal jack: stay pending (hover already showed it red)
    }

    if (!key) return;
    const source = endpointFromJackKey(key);
    const srcPoint = jackAnchors[key];
    if (!source || !srcPoint) return;
    e.preventDefault();
    cableDown = { x: e.clientX, y: e.clientY, moved: false };
    dragCable = { source, srcPoint, free: srcPoint, over: false, legal: false, verdict: null, mode: "drag" };
  }

  // While dragging or holding a pending cable: track the free end, re-derive the source anchor if it's in
  // view, and if the cursor is over another jack, evaluate legality (snapping the end to it for a magnetic
  // feel). Fires with no button pressed too, so a pending cable tracks the cursor between clicks.
  function onCablePointerMove(e: PointerEvent): void {
    if (!dragCable || !worldApi) return;
    if (dragCable.mode === "drag" && !cableDown.moved) {
      if (Math.hypot(e.clientX - cableDown.x, e.clientY - cableDown.y) > 4) cableDown.moved = true;
    }
    const cursor = worldApi.clientToSurface(e.clientX, e.clientY);
    // Live source anchor: if the source is (still/again) in view, use its measured jack; else keep the
    // pick-time point (it's off-view — the lead is drawn as a floating end, not a line to nowhere).
    const srcPoint = jackAnchors[jackKeyOf(dragCable.source)] ?? dragCable.srcPoint;
    const el = document
      .elementFromPoint(e.clientX, e.clientY)
      ?.closest<HTMLElement>("[data-jack]");
    const key = el?.dataset.jack;
    if (key && key !== jackKeyOf(dragCable.source)) {
      const target = endpointFromJackKey(key);
      if (target) {
        const verdict = evaluateConnection(dragCable.source, target, scene.patch.connections);
        dragCable = {
          ...dragCable,
          srcPoint,
          free: jackAnchors[key] ?? cursor,
          over: true,
          legal: verdict.ok,
          verdict,
        };
        return;
      }
    }
    dragCable = { ...dragCable, srcPoint, free: cursor, over: false, legal: false, verdict: null };
  }

  // Release. A drag over a legal jack commits; a drag that never moved (a click) is promoted to a
  // **pending** pick that survives a view switch; a real drag released over nothing is cancelled. A
  // pending cable is completed on the next pointer-*down*, so pointer-up does nothing for it.
  function onCablePointerUp(): void {
    if (!dragCable || dragCable.mode !== "drag") return;
    if (dragCable.verdict?.ok) {
      commitCable(dragCable.verdict);
      dragCable = null;
    } else if (!cableDown.moved) {
      dragCable = { ...dragCable, mode: "pending", over: false, legal: false, verdict: null };
    } else {
      dragCable = null;
    }
  }

  // Esc cancels an in-progress patch (drag or pending).
  function onCableKey(e: KeyboardEvent): void {
    if (e.key === "Escape" && dragCable) dragCable = null;
  }

  // The carrier domain of a connection (from its output port), or null if unknown.
  function connectionDomain(c: Connection): PortDomain | null {
    const dev = deviceById(c.from.device);
    const desc = dev ? descriptorFor(catalog, dev.typeId) : undefined;
    return desc?.ports.find((p) => p.direction === "output" && p.id === c.from.port)?.domain ?? null;
  }

  // The connector kind of a connection (from its output port) — picks the cable's colour from the signal
  // palette. Falls back to "line" (neutral grey) when the port can't be resolved.
  function connectionKind(c: Connection): PortKind {
    const dev = deviceById(c.from.device);
    const desc = dev ? descriptorFor(catalog, dev.typeId) : undefined;
    return desc?.ports.find((p) => p.direction === "output" && p.id === c.from.port)?.kind ?? "line";
  }

  // Apply a legal verdict to the patch and hot-swap: drop the replaced edge (fan-in is illegal, so a
  // new cable into an occupied input replaces its source), add the new one, rebuild the engine. A fresh
  // **analog** connection gets a transparent default cable (the first preset); digital/event stay ideal.
  function commitCable(v: ConnectVerdict): void {
    if (!v.ok) return;
    let conns = scene.patch.connections;
    if (v.replaces) {
      const rk = connKey(v.replaces);
      conns = conns.filter((c) => connKey(c) !== rk);
    }
    const conn: Connection = { from: v.connection.from, to: v.connection.to };
    if (connectionDomain(conn) === "analog" && cables[0]) conn.cable = cableSpec(cables[0]);
    scene.patch.connections = [...conns, conn];
    hotSwap();
  }

  // Remove a cable (from the inspector) and hot-swap. Anything it fed now reads silence.
  function disconnect(c: Connection): void {
    const k = connKey(c);
    scene.patch.connections = scene.patch.connections.filter((x) => connKey(x) !== k);
    if (selectedCableKey === k) selectedCableKey = null;
    hotSwap();
  }

  // --- Cable inspector (select a cable to change its type / disconnect it) --------------------------
  let selectedCableKey = $state<string | null>(null);
  // The selected connection, or null (also null once it's been disconnected).
  const selectedConn = $derived(
    scene.patch.connections.find((c) => connKey(c) === selectedCableKey) ?? null,
  );
  // The selected connection's loading loss in dB (from the static per-connection losses), or null for
  // a digital link / before the losses have arrived.
  const selectedLoss = $derived.by((): number | null => {
    const i = scene.patch.connections.findIndex((c) => connKey(c) === selectedCableKey);
    return i >= 0 ? (losses[i] ?? null) : null;
  });

  // Set (or clear, `""` ⇒ ideal wire) the cable type on a connection, then hot-swap — the cable's R·C
  // is baked into the edge at compile, so changing it rebuilds the engine.
  function setCableType(c: Connection, typeId: string): void {
    const idx = scene.patch.connections.findIndex((x) => connKey(x) === connKey(c));
    if (idx < 0) return;
    const preset = typeId ? cables.find((ct) => ct.typeId === typeId) : undefined;
    const updated: Connection = { from: { ...c.from }, to: { ...c.to } };
    if (preset) updated.cable = cableSpec(preset);
    scene.patch.connections[idx] = updated;
    hotSwap();
  }

  // The occupied U-runs of a rack, excluding `excludeId` (the device being placed).
  function rackOccupants(rackId: string, excludeId: string) {
    const occ: { startSlot: number; rackUnits: number }[] = [];
    for (const d of scene.patch.devices) {
      if (d.id === excludeId) continue;
      const place = scene.ui.placements[d.id];
      if (place?.rack?.id === rackId) {
        occ.push({ startSlot: place.rack.uSlot, rackUnits: deviceUnits(d.typeId) });
      }
    }
    return occ;
  }

  // If elevation `(x,y)` lands over an open rack on the shown wall, the nearest free start-slot a
  // `units`-high device fits at — else null. The drag-snap target. `(x,y)` are elevation coords (the
  // rack is compared via its projected frame), so this works identically on every wall.
  function rackSlotAt(
    excludeId: string,
    x: number,
    y: number,
    units: number,
  ): { rackId: string; slot: number } | null {
    for (const rack of scene.ui.racks) {
      if (rack.space !== currentSpace || rack.wall !== currentWall) continue;
      const frame = rackRect(rack);
      const slotOy = frame.y + FRAME_MARGIN;
      const within =
        x >= frame.x &&
        x <= frame.x + frame.width &&
        y >= slotOy &&
        y <= slotOy + rack.slots * RACK_UNIT_MM;
      if (!within) continue;
      const desired = Math.floor((y - slotOy) / RACK_UNIT_MM);
      const slot = nearestFreeSlot({ slots: rack.slots }, rackOccupants(rack.id, excludeId), units, desired);
      if (slot !== null) return { rackId: rack.id, slot };
    }
    return null;
  }

  // Legality for live drag feedback + the commit gate, in the shown wall's elevation. Racks reposition
  // freely; a device is legal if it can mount in a rack at `(x,y)`, or stands free without overlapping
  // any other item. The candidate's elevation width is always the panel width (a unit faces the room).
  function canPlace(id: string, x: number, y: number): boolean {
    if (isRack(id)) return true;
    const device = deviceById(id);
    if (!device) return false;
    const desc = descriptorFor(catalog, device.typeId);
    if (!desc) return false;
    const units = deviceUnits(device.typeId);
    if (units > 0 && rackSlotAt(id, x, y, units)) return true;
    const size = footprint(desc.formFactor);
    const candidate: Rect2 = { x, y, width: size.width, height: size.height };
    return !placedItems.some((it) => it.id !== id && rectsOverlap(candidate, it.rect));
  }

  // Commit a drag (only ever called for a legal spot), in the shown wall's elevation: move a rack, or
  // snap a device into a rack slot / set it free-standing. Elevation `(x,y)` is mapped back to the world
  // 3-D truth via `elevationToWorld` (so mirrored/rotated walls land where the cursor is).
  function moveTo(id: string, x: number, y: number): void {
    if (!currentWall) return; // dragging only happens in a wall elevation
    const rack = rackById(id);
    if (rack) {
      rack.position = elevationToWorld(rack.position, rackFrameSize(rack), rack.wall, room, x, y);
      return;
    }
    const device = deviceById(id);
    const place = scene.ui.placements[id];
    if (!device || !place) return;
    const units = deviceUnits(device.typeId);
    const hit = units > 0 ? rackSlotAt(id, x, y, units) : null;
    if (hit) {
      const rack = rackById(hit.rackId);
      place.rack = { id: hit.rackId, uSlot: hit.slot };
      if (rack) {
        place.space = rack.space; // a mounted device lives in its rack's space…
        place.wall = rack.wall; // …and against its rack's wall
      }
    } else {
      const desc = descriptorFor(catalog, device.typeId);
      const size = desc ? footprint(desc.formFactor) : { width: 0, height: 0, depth: 0 };
      place.rack = undefined;
      place.position = elevationToWorld(place.position, size, place.wall, room, x, y);
    }
  }

  // Spaces (rooms). Switching shows only that space's gear; membership persists in the scene.
  function addSpace(): void {
    let n = scene.ui.spaces.length + 1;
    while (scene.ui.spaces.some((s) => s.id === `space-${n}`)) n++;
    const space = newSpace(`space-${n}`, `Space ${n}`);
    scene.ui.spaces.push(space);
    currentSpace = space.id;
  }
  // Send a free-standing device to another space (it lands at that space's floor origin).
  function moveDeviceToSpace(id: string, spaceId: string): void {
    const place = scene.ui.placements[id];
    if (!place) return;
    place.rack = undefined;
    place.space = spaceId;
    place.position = { x: 0, y: 0, z: 0 };
  }
  // Move a rack to another space; its mounted gear follows.
  function moveRackToSpace(id: string, spaceId: string): void {
    const rack = rackById(id);
    if (!rack) return;
    rack.space = spaceId;
    for (const d of scene.patch.devices) {
      const place = scene.ui.placements[d.id];
      if (place?.rack?.id === id) place.space = spaceId;
    }
  }

  // Flip a unit front↔back to reach its rear I/O (no clearance step — flipping is direct).
  function toggleFlip(id: string): void {
    const place = scene.ui.placements[id];
    if (!place) return;
    place.facing = place.facing === "back" ? "front" : "back";
  }

  function seedParamValues(): void {
    const values: Record<string, number> = {};
    for (const device of scene.patch.devices) {
      const desc = descriptorFor(catalog, device.typeId);
      if (!desc) continue;
      for (const p of desc.params) {
        const saved = device.params?.find((s) => s.id === p.id)?.value;
        values[key(device.id, p.id)] = saved ?? p.default;
      }
    }
    paramValues = values;
  }

  function onParamInput(device: string, p: ParamDescriptor, value: number): void {
    paramValues[key(device, p.id)] = value;
    setSceneParam(scene, device, p.id, value); // keep the scene in sync for save
    send?.({ type: "param", device, paramId: p.id, value });
  }

  // Push every device's current param values to the engine — after a (re)build the host re-applies the
  // scene's control values over the queue (they'd glide from the node defaults otherwise).
  function pushParams(sendFn: (msg: ControlMessage) => void): void {
    for (const device of scene.patch.devices) {
      const desc = descriptorFor(catalog, device.typeId);
      if (!desc) continue;
      for (const p of desc.params) {
        sendFn({ type: "param", device: device.id, paramId: p.id, value: paramValue(device.id, desc, p.id) });
      }
    }
  }

  // A structural edit → rebuild the engine from the new patch (compile + ScheduleSlot hot-swap, in the
  // worklet, the Story 4.1 path) and re-apply param values. Edits are rare gestures, so the off-block
  // compile cost is acceptable; the live audio thread swaps at a block boundary.
  function hotSwap(): void {
    if (!send) return;
    send({ type: "loadPatch", patch: plainPatch() });
    seedParamValues();
    pushParams(send);
  }

  // A world placement for new gear spawned against the wall currently in view, at elevation-x `elevX`
  // (its perpendicular-to-wall axis sits it flush to that wall). Mapped back through `elevationToWorld`
  // so it appears at `elevX` on any wall, mirrored ones included. Falls back to the front wall in the
  // top view.
  function wallSpawn(size: Size3, elevX: number): { wall: Wall; position: Vec3 } {
    const wall = currentWall ?? "front";
    const FLUSH = 400; // nominal depth of the against-the-wall zone, world mm
    const seed: Vec3 =
      wall === "front"
        ? { x: 0, y: 0, z: room.depth - FLUSH }
        : wall === "right"
          ? { x: room.width - FLUSH, y: 0, z: 0 }
          : { x: 0, y: 0, z: 0 }; // back / left sit against the origin walls
    return { wall, position: elevationToWorld(seed, size, wall, room, elevX, 0) };
  }

  // Add gear from the catalog: a new instance placed free-standing on the wall in view (just past the
  // existing gear), then a hot-swap. Its ports read silence until patched (Story 4.4).
  function addDevice(typeId: string): void {
    const rightX = placedItems.reduce((m, it) => Math.max(m, it.rect.x + it.rect.width), 0);
    let n = 1;
    while (scene.patch.devices.some((d) => d.id === `${typeId}-${n}`)) n++;
    const id = `${typeId}-${n}`;
    const desc = descriptorFor(catalog, typeId);
    const size = desc ? footprint(desc.formFactor) : { width: 0, height: 0, depth: 0 };
    const { wall, position } = wallSpawn(size, rightX + 60);
    scene.patch.devices.push({ id, typeId });
    scene.ui.placements[id] = { space: currentSpace, wall, position, facing: "front" };
    hotSwap();
  }

  // Remove a device (never the output tap, which would invalidate the patch): drop it from the patch,
  // its connections, and its placement, then hot-swap. Anything it fed now reads silence.
  function removeDevice(id: string): void {
    if (scene.patch.output.device === id) return;
    scene.patch.devices = scene.patch.devices.filter((d) => d.id !== id);
    scene.patch.connections = scene.patch.connections.filter(
      (c) => c.from.device !== id && c.to.device !== id,
    );
    delete scene.ui.placements[id];
    hotSwap();
  }

  // Add / remove a rack — purely UI furniture (the engine has no racks), so no hot-swap. Removing a
  // rack un-mounts its gear, leaving each unit free-standing.
  function addRack(): void {
    const rightX = placedItems.reduce((m, it) => Math.max(m, it.rect.x + it.rect.width), 0);
    let n = 1;
    while (scene.ui.racks.some((r) => r.id === `rack-${n}`)) n++;
    const slots = 8;
    const frameSize: Size3 = {
      width: RACK_WIDTH_MM + 2 * FRAME_MARGIN,
      height: slots * RACK_UNIT_MM + 2 * FRAME_MARGIN,
      depth: RACK_DEPTH_MM,
    };
    const { wall, position } = wallSpawn(frameSize, rightX + 60);
    scene.ui.racks.push({ id: `rack-${n}`, space: currentSpace, wall, position, slots });
  }
  function removeRack(id: string): void {
    for (const d of scene.patch.devices) {
      const place = scene.ui.placements[d.id];
      if (place?.rack?.id === id) place.rack = undefined; // un-mount; keep its free position
    }
    scene.ui.racks = scene.ui.racks.filter((r) => r.id !== id);
  }

  async function start(): Promise<void> {
    if (started) return;
    started = true;
    try {
      const control = await startEngine(
        plainPatch(),
        {
          onStatus: (m) => {
          status = m;
        },
        onHealth: (h) => {
          health = healthSummary(h);
        },
        onLevel: (peak) => {
          level = peak;
        },
        onReadouts: (r) => {
          readings = Object.fromEntries(r);
        },
        onLosses: (l) => {
          losses = l;
        },
        onReady: (r: ReadyMessage, sendFn) => {
          catalog = r.catalog;
          cables = r.cables;
          losses = r.losses;
          send = sendFn;
          ready = true;
          seedParamValues();
          pushParams(sendFn); // match the engine to the scene from the first interaction
          if (synthDevice) {
            wireKeyboard(sendFn, synthDevice.id);
            wireMidi(sendFn, synthDevice.id, (m) => {
              midiStatus = m;
            });
          }
        },
        },
        volume,
      );
      setVolume = control.setVolume;
    } catch (err) {
      status = `error: ${err}`;
      started = false;
    }
  }

  function saveCurrent(): void {
    saveScene(scene);
    status = "scene saved";
  }

  function loadSaved(): void {
    const loaded = loadScene();
    if (!loaded) {
      status = "no saved scene";
      return;
    }
    scene = loaded;
    currentSpace = loaded.ui.spaces[0]?.id ?? "";
    seedParamValues();
    send?.({ type: "loadPatch", patch: plainPatch() }); // hot-swap the engine to the saved scene
    status = "scene loaded";
  }

  function reload(): void {
    send?.({ type: "loadPatch", patch: plainPatch() }); // re-apply current scene — proves glitch-free swap
    status = "scene reloaded (hot-swap)";
  }
</script>

<!-- Cable-drag pointer tracking: a jack press starts a cable; move/up run globally so the drag keeps
     working past the jack. WorldView's own pan/device-drag handlers stay inert (no jack ⇒ no cable drag,
     and no device drag/pan is active during a cable drag). -->
<svelte:window
  onpointerdown={onCablePointerDown}
  onpointermove={onCablePointerMove}
  onpointerup={onCablePointerUp}
  onkeydown={onCableKey}
/>

<main>
  <header class="toolbar">
    <button type="button" class="start" onclick={start} disabled={started}>▶ start</button>
    {#if ready}
      <div class="spaces">
        {#each scene.ui.spaces as space (space.id)}
          <button
            type="button"
            class="space-tab"
            class:active={space.id === currentSpace}
            onclick={() => (currentSpace = space.id)}
          >
            {space.name}
          </button>
        {/each}
        <button type="button" class="space-tab add" onclick={addSpace}>+ space</button>
      </div>

      <!-- Wall-view switcher: turn to face each wall of the room (top-down floor plan → Story 4.6.4). -->
      <div class="views" role="group" aria-label="wall view">
        {#each ["front", "right", "back", "left"] as const as w (w)}
          <button
            type="button"
            class="view-tab"
            class:active={currentView === w}
            onclick={() => (currentView = w)}
          >
            {WALL_LABELS[w]}
          </button>
        {/each}
      </div>

      <div class="palette">
        <span class="palette-label">Add:</span>
        {#each catalog as desc (desc.typeId)}
          <button type="button" class="add-chip" onclick={() => addDevice(desc.typeId)}>
            {desc.name}
          </button>
        {/each}
        <button type="button" class="add-chip rack" onclick={addRack}>Rack</button>
      </div>

      <div class="master">
        <label class="volume">
          <span>Vol</span>
          <input
            type="range"
            min="0"
            max="1"
            step="0.01"
            value={volume}
            oninput={(e) => onVolume(Number(e.currentTarget.value))}
          />
          <span class="readout">{Math.round(volume * 100)}%</span>
        </label>
        <Vu {level} />
      </div>

      <span class="scene-buttons">
        <button type="button" onclick={saveCurrent}>save</button>
        <button type="button" onclick={loadSaved}>load</button>
        <button type="button" onclick={reload}>reload</button>
      </span>

      <span class="statuses">{[status, health, midiStatus].filter(Boolean).join(" · ")}</span>
    {/if}
  </header>

  {#if ready}
    <div class="stage">
      <!-- One patch cable: three stacked strokes for depth (dark drop-shadow, signal-coloured core, thin
           lit highlight — colour from the connector kind), plus a wide transparent hit-path for
           click-to-disconnect (its pointer events go off during a drag so `elementFromPoint` can see the
           jack beneath). Drawn once, in the single cable layer; the devices' `z` handle the occlusion. -->
      {#snippet oneCable(c: Connection, api: WorldApi)}
        {@const a = cableAnchor(c.from, "output", api)}
        {@const b = cableAnchor(c.to, "input", api)}
        {#if a && b}
          {@const d = cablePathData(a, b)}
          {@const kind = connectionKind(c)}
          <path class="cable-shadow" {d} />
          <path
            class="cable-core"
            data-signal={kind}
            class:selected={connKey(c) === selectedCableKey}
            {d}
          />
          <path class="cable-highlight" data-signal={kind} {d} />
          <path
            class="cable-hit"
            {d}
            role="button"
            tabindex="-1"
            aria-label={`select cable ${connKey(c)}`}
            style:pointer-events={dragCable ? "none" : "stroke"}
            onclick={() => (selectedCableKey = connKey(c))}
            onkeydown={(e) => {
              if (e.key === "Enter" || e.key === " ") selectedCableKey = connKey(c);
            }}
          ></path>
        {/if}
      {/snippet}

      <!-- A cross-view connection: only one end is in this view (other wall or other room), so instead of
           a continuous cable we draw a short stub from that end to a labelled portal chip pointing at
           where the other end lives (the snakes MVP). The engine still sees a plain mono connection. -->
      {#snippet onePortal(c: Connection, api: WorldApi)}
        {@const fromIn = inView(c.from.device)}
        {@const ref = fromIn ? c.from : c.to}
        {@const dir = fromIn ? "output" : "input"}
        {@const otherLabel = otherEndLabel(fromIn ? c.to.device : c.from.device)}
        {@const a = cableAnchor(ref, dir, api)}
        {#if a}
          {@const p = { x: a.x + (fromIn ? PORTAL_LEN : -PORTAL_LEN), y: a.y + 36 }}
          {@const d = cablePathData(a, p)}
          <path
            class="cable-hit"
            {d}
            role="button"
            tabindex="-1"
            aria-label={`select cable ${connKey(c)}`}
            style:pointer-events={dragCable ? "none" : "stroke"}
            onclick={() => (selectedCableKey = connKey(c))}
            onkeydown={(e) => {
              if (e.key === "Enter" || e.key === " ") selectedCableKey = connKey(c);
            }}
          ></path>
          <path class="cable portal" class:selected={connKey(c) === selectedCableKey} {d} />
          <circle class="portal-dot" cx={p.x} cy={p.y} r="16" />
          <text
            class="portal-label"
            x={fromIn ? p.x + 26 : p.x - 26}
            y={p.y}
            text-anchor={fromIn ? "start" : "end"}
            dominant-baseline="middle"
          >
            {fromIn ? `→ ${otherLabel}` : `${otherLabel} →`}
          </text>
        {/if}
      {/snippet}

      <WorldView
        items={placedItems}
        onMoveTo={moveTo}
        {canPlace}
        fitKey={`${currentSpace}:${currentView}`}
        gridStep={GRID_MM}
        bind:api={worldApi}
      >
        {#snippet cables(api)}
          <!-- Every cable with both ends in this view, drawn once as a continuous lead. The devices' z
               (set in placedItems by facing) decides which panels each cable passes in front of vs behind. -->
          {#each scene.patch.connections.filter(bothInView) as c (connKey(c))}
            {@render oneCable(c, api)}
          {/each}
        {/snippet}

        {#snippet overlay(api)}
          <!-- Decorative window to the live room, on the front wall (a room detail — not a functional
               portal; cross-space audio rides the existing 4.4 portal cables). -->
          {#if currentView === "front"}
            {@const wTop = api.worldToSurface(room.width / 2 - 600, 1000)}
            {@const wBot = api.worldToSurface(room.width / 2 + 600, 500)}
            <g class="window">
              <rect x={wTop.x} y={wTop.y} width={wBot.x - wTop.x} height={wBot.y - wTop.y} rx="6" />
              <line x1={(wTop.x + wBot.x) / 2} y1={wTop.y} x2={(wTop.x + wBot.x) / 2} y2={wBot.y} />
              <line x1={wTop.x} y1={(wTop.y + wBot.y) / 2} x2={wBot.x} y2={(wTop.y + wBot.y) / 2} />
              <text class="window-label" x={(wTop.x + wBot.x) / 2} y={wTop.y - 22} text-anchor="middle">
                Live Room
              </text>
            </g>
          {/if}
          <!-- On top of everything: cross-view portal stubs and the drag rubber-band while patching. -->
          {#each scene.patch.connections.filter(oneInView) as c (connKey(c))}
            {@render onePortal(c, api)}
          {/each}
          {#if dragCable}
            {#if dragCable.mode === "drag" || inView(dragCable.source.device)}
              <!-- Source visible: draw the rubber-band from its jack to the cursor. -->
              <path
                class="cable dragging"
                class:pending={dragCable.mode === "pending"}
                class:legal={dragCable.over && dragCable.legal}
                class:illegal={dragCable.over && !dragCable.legal}
                d={cablePathData(dragCable.srcPoint, dragCable.free)}
              />
            {:else}
              <!-- Pending across a view switch: the source is off-view, so just track a floating end
                   (the banner names where it came from; hovering a jack colours it legal/illegal). -->
              <circle
                class="pending-end"
                class:legal={dragCable.over && dragCable.legal}
                class:illegal={dragCable.over && !dragCable.legal}
                cx={dragCable.free.x}
                cy={dragCable.free.y}
                r="12"
              />
            {/if}
          {/if}
        {/snippet}

        {#snippet controls(itemId)}
          {#if isRack(itemId)}
            {@const rack = rackById(itemId)}
            {#if rack}
              <select
                class="space-select"
                aria-label="rack space"
                value={rack.space}
                onchange={(e) => moveRackToSpace(itemId, e.currentTarget.value)}
              >
                {#each scene.ui.spaces as s (s.id)}
                  <option value={s.id}>{s.name}</option>
                {/each}
              </select>
              <button type="button" class="chip" aria-label="remove rack" onclick={() => removeRack(itemId)}>
                ✕
              </button>
            {/if}
          {:else}
            {@const place = scene.ui.placements[itemId]}
            {#if place}
              <button type="button" class="chip" onclick={() => toggleFlip(itemId)}>
                {place.facing === "back" ? "front" : "back"}
              </button>
              {#if !place.rack}
                <!-- Mounted gear follows its rack's space, so the selector only shows when free-standing. -->
                <select
                  class="space-select"
                  aria-label="device space"
                  value={place.space}
                  onchange={(e) => moveDeviceToSpace(itemId, e.currentTarget.value)}
                >
                  {#each scene.ui.spaces as s (s.id)}
                    <option value={s.id}>{s.name}</option>
                  {/each}
                </select>
              {/if}
              {#if scene.patch.output.device !== itemId}
                <!-- The output tap can't be removed (it would invalidate the patch). -->
                <button
                  type="button"
                  class="chip"
                  aria-label="remove device"
                  onclick={() => removeDevice(itemId)}
                >
                  ✕
                </button>
              {/if}
            {/if}
          {/if}
        {/snippet}

        {#snippet item(itemId)}
          {#if isRack(itemId)}
            {@const rack = rackById(itemId)}
            {#if rack}
              <div class="rack-frame">
                <span class="rack-label">{rack.id} · {rack.slots}U</span>
                <div class="slots">
                  {#each Array.from({ length: rack.slots }, (_, i) => i) as i (i)}
                    <div class="slot"></div>
                  {/each}
                </div>
              </div>
            {/if}
          {:else}
            {@const device = deviceById(itemId)}
            {@const desc = device ? descriptorFor(catalog, device.typeId) : undefined}
            {@const place = scene.ui.placements[itemId]}
            {#if device && desc && place}
              <Panel
                device={device.id}
                typeId={device.typeId}
                name={desc.name}
                params={desc.params}
                ports={desc.ports}
                readouts={desc.readouts}
                flipped={place.facing === "back"}
                valueFor={(id) => paramValue(device.id, desc, id)}
                readingFor={(id) => readingFor(device.id, id)}
                onParam={(p, v) => onParamInput(device.id, p, v)}
              >
                {#if device.typeId === "synth_voice"}
                  <!-- Synth-specific screen: ADSR contour from params 1=attack, 2=decay, 3=sustain, 4=release. -->
                  <Screen
                    attackMs={paramValue(device.id, desc, 1)}
                    decayMs={paramValue(device.id, desc, 2)}
                    sustain={paramValue(device.id, desc, 3)}
                    releaseMs={paramValue(device.id, desc, 4)}
                  />
                {/if}
              </Panel>
            {/if}
          {/if}
        {/snippet}
      </WorldView>

      {#if pendingSourceName}
        <!-- Cross-view patch in progress: a cable end is held from a click on a source jack. Turn to
             another wall/room and click a destination jack to complete (Esc / Cancel to drop it). -->
        <div class="patch-banner">
          Patching from <strong>{pendingSourceName}</strong> — click a destination jack
          <button type="button" onclick={() => (dragCable = null)}>Cancel</button>
        </div>
      {/if}

      {#if selectedConn}
        <!-- Cable inspector: click a cable to select it, then change its type or disconnect it. Only
             analog links carry a cable (R·C); digital/event links are always ideal. -->
        <div class="cable-inspector">
          <span class="ci-label">
            Cable <strong>{selectedConn.from.device}</strong> → <strong>{selectedConn.to.device}</strong>
          </span>
          {#if connectionDomain(selectedConn) === "analog"}
            <label class="ci-type">
              Type
              <select
                value={cableTypeIdFor(cables, selectedConn.cable)}
                onchange={(e) => selectedConn && setCableType(selectedConn, e.currentTarget.value)}
              >
                <option value="">Ideal wire</option>
                {#each cables as ct (ct.typeId)}
                  <option value={ct.typeId}>{ct.label}</option>
                {/each}
              </select>
            </label>
            <!-- Static impedance loading loss (the §5.3 divider), not a live meter — how far the loaded
                 input sits below the source's open-circuit voltage. -->
            <span class="ci-loss">
              loading {selectedLoss !== null ? `${selectedLoss.toFixed(2)} dB` : "—"}
            </span>
          {:else}
            <span class="ci-ideal">digital link — ideal (no cable)</span>
          {/if}
          <button type="button" onclick={() => selectedConn && disconnect(selectedConn)}>
            Disconnect
          </button>
          <button type="button" class="ci-close" onclick={() => (selectedCableKey = null)}>Close</button>
        </div>
      {/if}

      <!-- Global levels & losses: gain-staging across the chain in one place — every meter device's
           live readings, and each analog connection's static loading loss (the §5.3 divider). The
           MIDI status + scene buttons now live in the header (design-system layout). -->
      <details class="levels">
        <summary>Signal path — levels &amp; losses</summary>
        <div class="levels-body">
          <ul class="meter-list">
            {#each scene.patch.devices as d (d.id)}
              {@const desc = descriptorFor(catalog, d.typeId)}
              {#if desc && desc.readouts.length > 0}
                <li>
                  <span class="dev">{desc.name}</span>
                  {#each desc.readouts as r (r.id)}
                    <span class="reading">
                      {r.label} <strong>{fmtReading(readingFor(d.id, r.id))}</strong>
                      {r.unit}
                    </span>
                  {/each}
                </li>
              {/if}
            {/each}
          </ul>
          <ul class="loss-list">
            {#each scene.patch.connections as c, i (connKey(c))}
              {@const loss = losses[i]}
              {#if loss !== undefined && loss !== null}
                <li>
                  {c.from.device} → {c.to.device}
                  <strong>{loss.toFixed(2)} dB</strong> loading
                </li>
              {/if}
            {/each}
          </ul>
        </div>
      </details>
    </div>
  {/if}
</main>

<style>
  main {
    font: 15px/1.5 var(--ae-font-ui);
    display: flex;
    flex-direction: column;
    height: 100dvh;
    color: var(--ae-text-secondary);
  }
  /* Slim top toolbar over a full-height stage; wraps to a second row if the palette gets wide. */
  .toolbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.4rem 0.8rem;
    padding: 0.4rem 0.7rem;
    background: var(--ae-bg-panel);
    border-bottom: 1px solid var(--ae-line-panel);
  }
  .start {
    font-weight: 600;
  }
  .statuses {
    margin-left: auto;
    font-size: 0.75rem;
    color: var(--ae-text-muted);
    font-variant-numeric: tabular-nums;
  }
  .stage {
    position: relative; /* anchor for the floating cable inspector */
    flex: 1;
    min-height: 0;
  }
  .master {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }
  .volume {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    font-size: 0.8rem;
    color: var(--ae-text-secondary);
  }
  .volume input {
    width: 12rem;
  }
  .volume .readout {
    width: 3rem;
    font-variant-numeric: tabular-nums;
    color: var(--ae-text-muted);
  }
  button {
    font: inherit;
    padding: 0.5em 1.2em;
    cursor: pointer;
    color: var(--ae-text-strong);
    background: var(--ae-bg-chip);
    border: 1px solid var(--ae-line-chip);
    border-radius: var(--ae-radius-control);
  }
  button:hover:not(:disabled) {
    background: var(--ae-bg-panel);
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .scene-buttons {
    display: flex;
    gap: 0.3rem;
  }
  .spaces {
    display: flex;
    gap: 0.3rem;
    flex-wrap: wrap;
  }
  /* Wall-view switcher — a segmented control to turn between the room's walls. */
  .views {
    display: flex;
    gap: 0;
    border: 1px solid var(--ae-line-chip);
    border-radius: var(--ae-radius-control);
    overflow: hidden;
  }
  .view-tab {
    font: inherit;
    font-size: 0.75rem;
    padding: 0.2rem 0.6rem;
    border: none;
    border-radius: 0;
    border-left: 1px solid var(--ae-line-chip);
    background: var(--ae-bg-panel-2);
    color: var(--ae-text-muted);
    cursor: pointer;
  }
  .view-tab:first-child {
    border-left: none;
  }
  .view-tab.active {
    background: var(--ae-bg-chip);
    color: var(--ae-text-primary);
  }
  .space-tab {
    font: inherit;
    font-size: 0.8rem;
    padding: 0.25rem 0.7rem;
    border: 1px solid var(--ae-line-chip);
    border-bottom: none;
    border-radius: 5px 5px 0 0;
    background: var(--ae-bg-panel-2);
    color: var(--ae-text-muted);
    cursor: pointer;
  }
  .space-tab.active {
    background: var(--ae-bg-panel);
    color: var(--ae-text-primary);
    border-color: var(--ae-line-panel);
  }
  .space-tab.add {
    color: var(--ae-text-faint);
    background: transparent;
    border-style: dashed;
  }
  .space-select {
    font: inherit;
    font-size: 9px;
    margin-left: 2px;
    max-width: 6rem;
    border: 1px solid var(--ae-line-chip);
    border-radius: 3px;
    background: var(--ae-bg-chip);
    color: var(--ae-text-strong);
  }
  .palette {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.3rem;
    margin: 0.3rem 0;
  }
  .palette-label {
    font-size: 0.75rem;
    color: var(--ae-text-muted);
  }
  .add-chip {
    font: inherit;
    font-size: 0.75rem;
    padding: 0.2rem 0.6rem;
    border: 1px solid var(--ae-line-chip);
    border-radius: 12px;
    background: var(--ae-bg-chip);
    color: var(--ae-text-strong);
    cursor: pointer;
  }
  .add-chip:hover {
    background: var(--ae-bg-panel);
  }
  .add-chip.rack {
    border-style: dashed;
    color: var(--ae-text-muted);
  }
  /* A patch cable drawn in the world overlay (surface mm; stroke scales with zoom). */
  .cable {
    fill: none;
    stroke: #d98c3c;
    stroke-width: 6;
    stroke-linecap: round;
    opacity: 0.9;
    pointer-events: none;
  }
  /* Wide invisible hit target so a thin cable is easy to click to disconnect. `tabindex=-1` keeps it
     out of the tab order, so suppressing the click-focus outline (its rectangular path bounding box)
     costs no keyboard accessibility. */
  .cable-hit {
    fill: none;
    stroke: transparent;
    stroke-width: 14;
    cursor: pointer;
    outline: none;
  }
  /* A settled patch lead: three stacked strokes (shadow / signal core / lit highlight), coloured by the
     connection's connector kind. Widths come from the cable tokens. */
  .cable-shadow,
  .cable-core,
  .cable-highlight {
    fill: none;
    stroke-linecap: round;
    pointer-events: none;
  }
  .cable-shadow {
    stroke: var(--ae-cable-shadow);
    stroke-width: var(--ae-cable-shadow-w);
    opacity: 0.5;
  }
  .cable-core {
    stroke: var(--ae-signal-line);
    stroke-width: var(--ae-cable-core-w);
  }
  .cable-highlight {
    stroke: var(--ae-signal-line-lit);
    stroke-width: var(--ae-cable-highlight-w);
    opacity: 0.6;
  }
  .cable-core[data-signal="mic"] {
    stroke: var(--ae-signal-mic);
  }
  .cable-core[data-signal="instrument"] {
    stroke: var(--ae-signal-instrument);
  }
  .cable-core[data-signal="speaker"] {
    stroke: var(--ae-signal-speaker);
  }
  .cable-core[data-signal="digital"] {
    stroke: var(--ae-signal-digital);
  }
  .cable-core[data-signal="midi"] {
    stroke: var(--ae-signal-midi);
  }
  .cable-highlight[data-signal="mic"] {
    stroke: var(--ae-signal-mic-lit);
  }
  .cable-highlight[data-signal="instrument"] {
    stroke: var(--ae-signal-instrument-lit);
  }
  .cable-highlight[data-signal="speaker"] {
    stroke: var(--ae-signal-speaker-lit);
  }
  .cable-highlight[data-signal="digital"] {
    stroke: var(--ae-signal-digital-lit);
  }
  .cable-highlight[data-signal="midi"] {
    stroke: var(--ae-signal-midi-lit);
  }
  /* Selected lead: fatten the core so the inspector target reads clearly. */
  .cable-core.selected {
    stroke-width: calc(var(--ae-cable-core-w) + 3px);
  }
  /* The rubber-band while dragging a new cable. */
  .cable.dragging {
    stroke-dasharray: 12 9;
    opacity: 0.85;
  }
  .cable.legal {
    stroke: #4caf50;
  }
  .cable.illegal {
    stroke: #d9534f;
  }
  /* A pending (clicked-and-held) cable lead — a lighter dash so it reads as "held", not being dragged. */
  .cable.pending {
    opacity: 0.6;
  }
  /* The floating end of a pending cable when its source is on another wall/room (no line to draw). */
  .pending-end {
    fill: #d98c3c;
    stroke: #1b1d20;
    stroke-width: 3;
    opacity: 0.9;
  }
  .pending-end.legal {
    fill: #4caf50;
  }
  .pending-end.illegal {
    fill: #d9534f;
  }
  /* The currently-selected cable (its inspector is open). */
  .cable.selected {
    stroke: #f4a94a;
    stroke-width: 9;
    opacity: 1;
  }
  /* Decorative window to the live room on the front wall (glass pane + mullions + label). */
  .window rect {
    fill: rgba(120, 170, 200, 0.1);
    stroke: var(--ae-line-hard);
    stroke-width: 4;
  }
  .window line {
    stroke: var(--ae-line-hard);
    stroke-width: 3;
  }
  .window-label {
    fill: var(--ae-text-muted);
    font-size: 30px;
    letter-spacing: var(--ae-legend-spacing);
    text-transform: uppercase;
  }
  /* A cross-space portal stub + its chip and room label. */
  .cable.portal {
    stroke-dasharray: 4 10;
  }
  .portal-dot {
    fill: #d98c3c;
    stroke: #1b1d20;
    stroke-width: 3;
  }
  .portal-label {
    fill: #e0e0e0;
    font-size: 34px;
    font-weight: 600;
    paint-order: stroke;
    stroke: #1b1d20;
    stroke-width: 5;
  }
  /* Cross-view patch banner — floats top-centre while a cable end is held (clicked from a source jack). */
  .patch-banner {
    position: absolute;
    left: 50%;
    top: 1rem;
    transform: translateX(-50%);
    z-index: 6;
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.4rem 0.75rem;
    background: var(--ae-bg-panel);
    color: var(--ae-text-strong);
    border: 1px solid var(--ae-signal-line-lit);
    border-radius: var(--ae-radius-panel);
    box-shadow: var(--ae-shadow-card);
    font-size: 0.8rem;
  }
  .patch-banner button {
    font: inherit;
    font-size: 0.72rem;
    padding: 0.2rem 0.7rem;
  }
  /* Cable inspector strip (shown when a cable is selected). */
  /* Floats over the stage (bottom-centre) rather than taking layout space, so the world stays full. */
  .cable-inspector {
    position: absolute;
    left: 50%;
    bottom: 1rem;
    transform: translateX(-50%);
    z-index: 6;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.6rem;
    padding: 0.4rem 0.75rem;
    background: var(--ae-bg-panel);
    color: var(--ae-text-strong);
    border: 1px solid var(--ae-line-panel);
    border-radius: var(--ae-radius-panel);
    box-shadow: var(--ae-shadow-card);
    font-size: 0.8rem;
  }
  .cable-inspector .ci-type,
  .cable-inspector .ci-ideal {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    color: #b8bcc2;
  }
  .cable-inspector select {
    font: inherit;
    font-size: 0.75rem;
  }
  .cable-inspector button {
    font: inherit;
    font-size: 0.72rem;
    padding: 0.2rem 0.7rem;
  }
  .cable-inspector .ci-loss {
    color: #8fd0a0;
    font-variant-numeric: tabular-nums;
  }
  .cable-inspector .ci-close {
    margin-left: auto;
  }
  /* Global levels & losses panel — live meter readings + static connection losses. */
  .levels {
    margin: 0.6rem 0;
    font-size: 0.8rem;
    color: #444;
  }
  .levels summary {
    cursor: pointer;
    color: #666;
  }
  .levels-body {
    display: flex;
    flex-wrap: wrap;
    gap: 1rem 2rem;
    margin-top: 0.4rem;
  }
  .levels ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .levels li {
    padding: 0.1rem 0;
    font-variant-numeric: tabular-nums;
  }
  .levels .dev {
    display: inline-block;
    min-width: 7rem;
    color: #666;
  }
  .levels .reading {
    margin-right: 0.8rem;
    color: #555;
  }
  .levels .loss-list li {
    color: #777;
  }
  /* Small chrome buttons in a world item's top bar (device flip, space selector, remove). */
  .chip {
    font: inherit;
    font-size: 9px;
    line-height: 1;
    padding: 1px 5px;
    margin: 0 1px;
    border: 1px solid #555;
    border-radius: 3px;
    background: #4a4d52;
    color: #e0e0e0;
    cursor: pointer;
  }
  .chip:hover {
    background: #585c62;
  }
  /* A rack: a dark frame filling its world box, padded to inset the U-slot guide rows. */
  .rack-frame {
    width: 100%;
    height: 100%;
    box-sizing: border-box;
    border: 1px solid var(--ae-line-hard);
    border-radius: 9px;
    background: linear-gradient(var(--ae-rack-shell-1), var(--ae-rack-shell-2));
    box-shadow:
      var(--ae-shadow-rack),
      inset 0 1px 0 rgba(255, 255, 255, 0.05);
    padding: 14px; /* = FRAME_MARGIN, so guide rows align with mounted gear */
    position: relative;
  }
  /* Warm top-light wash — the room light hitting the top of the cabinet. */
  .rack-frame::before {
    content: "";
    position: absolute;
    top: 0;
    left: 14%;
    right: 14%;
    height: 70px;
    background: radial-gradient(closest-side at 50% 0, var(--ae-rack-glow), transparent);
    pointer-events: none;
    border-radius: 9px;
  }
  /* Perforated mounting rails: two columns of punched holes down the left/right margins. */
  .rack-frame::after {
    content: "";
    position: absolute;
    inset: 0;
    pointer-events: none;
    border-radius: 9px;
    background:
      radial-gradient(circle at 7px 15px, var(--ae-rack-hole) 1.8px, transparent 2.4px) 0 0 / 100% 30px
        repeat-y,
      radial-gradient(circle at calc(100% - 7px) 15px, var(--ae-rack-hole) 1.8px, transparent 2.4px) 0 0 /
        100% 30px repeat-y;
  }
  .rack-label {
    position: absolute;
    top: 3px;
    left: 16px;
    z-index: 1;
    font-family: var(--ae-font-ui);
    font-size: 8px;
    letter-spacing: var(--ae-legend-spacing);
    text-transform: uppercase;
    color: var(--ae-text-muted);
  }
  .slots {
    display: flex;
    flex-direction: column-reverse; /* slot 0 at the bottom, matching uSlot indexing */
    height: 100%;
  }
  .slot {
    flex: 1;
    border-bottom: 1px dashed var(--ae-line-panel);
  }
  .slot:first-child {
    border-bottom: none;
  }
</style>
