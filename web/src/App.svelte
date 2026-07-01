<script lang="ts">
  // The harness shell, now in Svelte 5. It owns the authoritative scene and the reactive UI state;
  // the engine/worklet bring-up and control transport live in engine.ts. Controls are rendered
  // **from the fetched device catalog** (not hardcoded ids) — a generic stepping stone; the
  // skeuomorphic panel widgets land in Story 4.2.3. Generic by device id throughout.

  import type { DeviceDescriptor, ParamDescriptor } from "./catalog";
  import { descriptorFor, isPlayable } from "./catalog";
  import {
    type ControlMessage,
    healthSummary,
    type ReadyMessage,
    startEngine,
    wireKeyboard,
    wireMidi,
  } from "./engine";
  import { cablePathData, evaluateConnection } from "./connections";
  import type { ConnectVerdict, Endpoint } from "./connections";
  import type { Connection, Patch, PortRef } from "./scene";
  import { defaultScene, loadScene, type Scene, saveScene, setSceneParam } from "./scene-store";
  import type { Rack } from "./scene-store";
  import {
    footprint,
    nearestFreeSlot,
    project,
    RACK_UNIT_MM,
    RACK_WIDTH_MM,
    type Rect2,
    rectsOverlap,
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
  let send = $state<((msg: ControlMessage) => void) | null>(null);
  // Master output peak (linear, ±1.0 = full scale), from the worklet's throttled level message.
  let level = $state(0);
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

  const FRAME_MARGIN = 14; // mm of rack frame drawn around the U-slot region

  const deviceById = (id: string) => scene.patch.devices.find((d) => d.id === id);
  const rackById = (id: string) => scene.ui.racks.find((r) => r.id === id);
  const isRack = (id: string) => rackById(id) !== undefined;

  // How many U a device occupies — 0 if it isn't rackmount gear (so it never mounts in a rack).
  function deviceUnits(typeId: string): number {
    const desc = descriptorFor(catalog, typeId);
    return desc && desc.formFactor.kind === "rackmount" ? desc.formFactor.rackUnits : 0;
  }

  // A rack's U-slot region origin (lower-left), world mm — inset from the frame by the margin.
  const slotOrigin = (rack: Rack) => ({ x: rack.position.x + FRAME_MARGIN, y: rack.position.y + FRAME_MARGIN });

  // A rack's frame rect (the draggable box drawn behind its gear).
  function rackRect(rack: Rack): Rect2 {
    return {
      x: rack.position.x,
      y: rack.position.y,
      width: RACK_WIDTH_MM + 2 * FRAME_MARGIN,
      height: rack.slots * RACK_UNIT_MM + 2 * FRAME_MARGIN,
    };
  }

  // A device's front-elevation rect — derived from its rack + U-slot when mounted, else from its
  // free-standing position. `null` if it has no descriptor or is hidden inside a collapsed rack.
  function deviceRect(deviceId: string, typeId: string): Rect2 | null {
    const desc = descriptorFor(catalog, typeId);
    const place = scene.ui.placements[deviceId];
    if (!desc || !place) return null;
    const size = footprint(desc.formFactor);
    if (place.rack) {
      const rack = rackById(place.rack.id);
      if (!rack) return null;
      const o = slotOrigin(rack);
      return { x: o.x, y: o.y + place.rack.uSlot * RACK_UNIT_MM, width: size.width, height: size.height };
    }
    return project(place.position, size, "front");
  }

  // The items to render in the current space: rack frames first (behind), then devices (on top).
  const placedItems = $derived([
    ...scene.ui.racks
      .filter((r) => r.space === currentSpace)
      .map((r) => ({ id: r.id, rect: rackRect(r) })),
    ...scene.patch.devices
      .filter((d) => scene.ui.placements[d.id]?.space === currentSpace)
      .map((d) => ({ id: d.id, rect: deviceRect(d.id, d.typeId) }))
      .filter((it): it is { id: string; rect: Rect2 } => it.rect !== null),
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
    if (!device || !place || place.space !== currentSpace) return null;
    if (place.facing === "back") {
      const jack = jackAnchors[jackKey(ref.device, direction, ref.port)];
      if (jack) return jack; // precise: the real socket on the shown back panel
    }
    const desc = descriptorFor(catalog, device.typeId);
    const rect = deviceRect(ref.device, device.typeId);
    if (!desc || !rect) return null;
    const ports = desc.ports.filter((p) => p.direction === direction);
    const idx = Math.max(
      0,
      ports.findIndex((p) => p.id === ref.port),
    );
    const frac = (idx + 1) / (ports.length + 1); // 0..1 down from the top
    const wx = direction === "output" ? rect.x + rect.width : rect.x;
    const wy = rect.y + rect.height * (1 - frac); // world y-up; port 0 nearest the top
    return api.worldToSurface(wx, wy);
  }

  // --- Drag-to-connect ------------------------------------------------------------------------------
  // An in-progress cable drag from a source jack: the fixed source end, the moving free end (surface
  // coords), and — when hovering a candidate jack — the verdict so we can colour the cable + commit.
  let dragCable = $state<{
    source: Endpoint;
    srcPoint: { x: number; y: number };
    free: { x: number; y: number };
    over: boolean;
    legal: boolean;
    verdict: ConnectVerdict | null;
  } | null>(null);

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

  // Pointer-down on a jack connector starts a cable drag. (Only reachable on a shown back panel; a
  // front-facing device's back is rotated away and non-interactive.)
  function onCablePointerDown(e: PointerEvent): void {
    if (!worldApi) return;
    const el = (e.target as HTMLElement | null)?.closest<HTMLElement>("[data-jack]");
    const key = el?.dataset.jack;
    if (!key) return;
    const source = endpointFromJackKey(key);
    const srcPoint = jackAnchors[key];
    if (!source || !srcPoint) return;
    e.preventDefault();
    dragCable = { source, srcPoint, free: srcPoint, over: false, legal: false, verdict: null };
  }

  // While dragging: track the free end, and if it's over another jack, evaluate legality (snapping the
  // cable end to that jack for a magnetic feel).
  function onCablePointerMove(e: PointerEvent): void {
    if (!dragCable || !worldApi) return;
    const cursor = worldApi.clientToSurface(e.clientX, e.clientY);
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
          free: jackAnchors[key] ?? cursor,
          over: true,
          legal: verdict.ok,
          verdict,
        };
        return;
      }
    }
    dragCable = { ...dragCable, free: cursor, over: false, legal: false, verdict: null };
  }

  // Release: commit a legal connection, else drop the drag (the rubber-band vanishes).
  function onCablePointerUp(): void {
    if (dragCable?.verdict?.ok) commitCable(dragCable.verdict);
    dragCable = null;
  }

  // Apply a legal verdict to the patch and hot-swap: drop the replaced edge (fan-in is illegal, so a
  // new cable into an occupied input replaces its source), add the new one, rebuild the engine.
  function commitCable(v: ConnectVerdict): void {
    if (!v.ok) return;
    let conns = scene.patch.connections;
    if (v.replaces) {
      const rk = connKey(v.replaces);
      conns = conns.filter((c) => connKey(c) !== rk);
    }
    scene.patch.connections = [...conns, v.connection];
    hotSwap();
  }

  // Remove a cable (click-to-disconnect) and hot-swap. Anything it fed now reads silence.
  function disconnect(c: Connection): void {
    const k = connKey(c);
    scene.patch.connections = scene.patch.connections.filter((x) => connKey(x) !== k);
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

  // If (x,y) lands over an open rack in this space, the nearest free start-slot a `units`-high device
  // fits at — else null. The drag-snap target.
  function rackSlotAt(
    excludeId: string,
    x: number,
    y: number,
    units: number,
  ): { rackId: string; slot: number } | null {
    for (const rack of scene.ui.racks) {
      if (rack.space !== currentSpace) continue;
      const o = slotOrigin(rack);
      const within =
        x >= o.x - FRAME_MARGIN &&
        x <= o.x + RACK_WIDTH_MM + FRAME_MARGIN &&
        y >= o.y &&
        y <= o.y + rack.slots * RACK_UNIT_MM;
      if (!within) continue;
      const desired = Math.floor((y - o.y) / RACK_UNIT_MM);
      const slot = nearestFreeSlot({ slots: rack.slots }, rackOccupants(rack.id, excludeId), units, desired);
      if (slot !== null) return { rackId: rack.id, slot };
    }
    return null;
  }

  // Legality for live drag feedback + the commit gate. Racks reposition freely; a device is legal if
  // it can mount in a rack at (x,y), or stands free without overlapping any other item.
  function canPlace(id: string, x: number, y: number): boolean {
    if (isRack(id)) return true;
    const device = deviceById(id);
    if (!device) return false;
    const desc = descriptorFor(catalog, device.typeId);
    if (!desc) return false;
    const units = deviceUnits(device.typeId);
    if (units > 0 && rackSlotAt(id, x, y, units)) return true;
    const candidate = project({ x, y, z: 0 }, footprint(desc.formFactor), "front");
    return !placedItems.some((it) => it.id !== id && rectsOverlap(candidate, it.rect));
  }

  // Commit a drag (only ever called for a legal spot): move a rack, or snap a device into a rack slot
  // / set it free-standing.
  function moveTo(id: string, x: number, y: number): void {
    const rack = rackById(id);
    if (rack) {
      rack.position.x = x;
      rack.position.y = y;
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
      if (rack) place.space = rack.space; // a mounted device lives in its rack's space
    } else {
      place.rack = undefined;
      place.position = { x, y, z: place.position.z };
    }
  }

  // Spaces (rooms). Switching shows only that space's gear; membership persists in the scene.
  function addSpace(): void {
    let n = scene.ui.spaces.length + 1;
    while (scene.ui.spaces.some((s) => s.id === `space-${n}`)) n++;
    const space = { id: `space-${n}`, name: `Space ${n}` };
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

  // Clearance + flip (gated): a unit must be pulled out before its back is reachable.
  function togglePulled(id: string): void {
    const place = scene.ui.placements[id];
    if (!place) return;
    place.pulledOut = !place.pulledOut;
    if (!place.pulledOut) place.facing = "front"; // pushing back in turns it front again
  }
  function toggleFlip(id: string): void {
    const place = scene.ui.placements[id];
    if (!place?.pulledOut) return; // gated behind clearance
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

  // Add gear from the catalog: a new instance placed free-standing in the current space (just past the
  // existing gear), then a hot-swap. Its ports read silence until patched (Story 4.4).
  function addDevice(typeId: string): void {
    const rightX = placedItems.reduce((m, it) => Math.max(m, it.rect.x + it.rect.width), 0);
    let n = 1;
    while (scene.patch.devices.some((d) => d.id === `${typeId}-${n}`)) n++;
    const id = `${typeId}-${n}`;
    scene.patch.devices.push({ id, typeId });
    scene.ui.placements[id] = {
      space: currentSpace,
      position: { x: rightX + 60, y: 0, z: 0 },
      facing: "front",
      pulledOut: false,
    };
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
    scene.ui.racks.push({ id: `rack-${n}`, space: currentSpace, position: { x: rightX + 60, y: 0, z: 0 }, slots: 8 });
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
        onReady: (r: ReadyMessage, sendFn) => {
          catalog = r.catalog;
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
/>

<main>
  <h1>Scene-driven engine — Svelte harness</h1>
  <p>
    The canonical <em>scene</em> (<code>synth → AD → DA → speaker</code>) built from a serialized
    patch and running live in an <code>AudioWorkletProcessor</code> as <code>SceneEngine</code>.
    Controls are rendered
    <strong>from the device catalog</strong>
    and addressed
    <strong>by device id</strong>; the scene can be <strong>saved / loaded</strong> (versioned JSON
    in localStorage) and
    <strong>reloaded live</strong> to exercise the engine's glitch-free hot-swap.
  </p>
  <p>
    <strong>Build the wasm first:</strong> <code>npm run wasm</code>, then <code>npm run dev</code>.
    Browsers require a user gesture to start audio.
  </p>

  <p><button type="button" onclick={start} disabled={started}>▶ start</button></p>
  <p class="status">{status}</p>
  <p class="health">{health}</p>

  {#if ready}
    <section class="controls">
      <div class="master">
        <label class="volume">
          <span>Volume</span>
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

      <p>
        Play with the keyboard: <kbd>A</kbd> <kbd>W</kbd> <kbd>S</kbd> <kbd>E</kbd> <kbd>D</kbd>
        <kbd>F</kbd> <kbd>T</kbd> <kbd>G</kbd> <kbd>Y</kbd> <kbd>H</kbd> <kbd>U</kbd> <kbd>J</kbd>
        <kbd>K</kbd> map to one octave from C4. (<kbd>Z</kbd>/<kbd>X</kbd> shift octave down/up.)
      </p>

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

      <div class="palette">
        <span class="palette-label">Add to {currentSpace}:</span>
        {#each catalog as desc (desc.typeId)}
          <button type="button" class="add-chip" onclick={() => addDevice(desc.typeId)}>
            {desc.name}
          </button>
        {/each}
        <button type="button" class="add-chip rack" onclick={addRack}>Rack</button>
      </div>
      <p class="world-hint">
        Drag the background to pan, scroll to zoom; drag a unit by its top bar to move it (snap into a
        rack's free U-slot, or out onto the floor; red = an illegal spot). To see a unit's back,
        <strong>pull it out</strong> first, then flip. Send a unit or rack to another room with its
        space selector. Zoom in to operate a panel.
      </p>
      <WorldView items={placedItems} onMoveTo={moveTo} {canPlace} fitKey={currentSpace} bind:api={worldApi}>
        {#snippet overlay(api)}
          <!-- Patch cables: one bezier per connection whose ends are both in the shown space. The wide
               transparent hit-path makes the cable clickable to disconnect; during a drag its pointer
               events are disabled so `elementFromPoint` can see the jack beneath it. -->
          {#each scene.patch.connections as c (connKey(c))}
            {@const a = cableAnchor(c.from, "output", api)}
            {@const b = cableAnchor(c.to, "input", api)}
            {#if a && b}
              {@const d = cablePathData(a, b)}
              <path
                class="cable-hit"
                {d}
                role="button"
                tabindex="-1"
                aria-label={`disconnect ${connKey(c)}`}
                style:pointer-events={dragCable ? "none" : "stroke"}
                onclick={() => disconnect(c)}
                onkeydown={(e) => {
                  if (e.key === "Enter" || e.key === " ") disconnect(c);
                }}
              ></path>
              <path class="cable" {d} />
            {/if}
          {/each}

          <!-- The rubber-band while dragging a new cable: green over a legal target, red over an
               illegal one, neutral in open space. -->
          {#if dragCable}
            <path
              class="cable dragging"
              class:legal={dragCable.over && dragCable.legal}
              class:illegal={dragCable.over && !dragCable.legal}
              d={cablePathData(dragCable.srcPoint, dragCable.free)}
            />
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
              <button type="button" class="chip" onclick={() => togglePulled(itemId)}>
                {place.pulledOut ? "push in" : "pull out"}
              </button>
              {#if place.pulledOut}
                <button type="button" class="chip" onclick={() => toggleFlip(itemId)}>
                  {place.facing === "back" ? "front" : "back"}
                </button>
              {/if}
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
                name={desc.name}
                params={desc.params}
                ports={desc.ports}
                flipped={place.facing === "back"}
                valueFor={(id) => paramValue(device.id, desc, id)}
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

      <p class="midi">{midiStatus}</p>
      <p class="scene-buttons">
        <button type="button" onclick={saveCurrent}>save scene</button>
        <button type="button" onclick={loadSaved}>load scene</button>
        <button type="button" onclick={reload}>reload (hot-swap)</button>
      </p>
    </section>
  {/if}
</main>

<style>
  main {
    font:
      15px/1.5 system-ui,
      sans-serif;
    max-width: 52rem;
    margin: 3rem auto;
    padding: 0 1rem;
    color: #1a1a1a;
  }
  .master {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    margin: 0.5rem 0 1rem;
  }
  .volume {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    font-size: 0.8rem;
    color: #444;
  }
  .volume input {
    width: 12rem;
  }
  .volume .readout {
    width: 3rem;
    font-variant-numeric: tabular-nums;
    color: #777;
  }
  code {
    background: #f0f0f0;
    padding: 0.1em 0.3em;
    border-radius: 3px;
  }
  button {
    font: inherit;
    padding: 0.5em 1.2em;
    cursor: pointer;
  }
  .status {
    color: #555;
  }
  .health {
    color: #777;
    font-size: 0.85em;
    font-variant-numeric: tabular-nums;
  }
  .controls {
    margin-top: 1.5rem;
  }
  .spaces {
    display: flex;
    gap: 0.3rem;
    margin: 0.5rem 0 0.3rem;
    flex-wrap: wrap;
  }
  .space-tab {
    font: inherit;
    font-size: 0.8rem;
    padding: 0.25rem 0.7rem;
    border: 1px solid #ccc;
    border-bottom: none;
    border-radius: 5px 5px 0 0;
    background: #ececec;
    color: #555;
    cursor: pointer;
  }
  .space-tab.active {
    background: #2a2d31;
    color: #fff;
    border-color: #2a2d31;
  }
  .space-tab.add {
    color: #888;
    background: transparent;
    border-style: dashed;
  }
  .space-select {
    font: inherit;
    font-size: 9px;
    margin-left: 2px;
    max-width: 6rem;
    border: 1px solid #555;
    border-radius: 3px;
    background: #4a4d52;
    color: #e0e0e0;
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
    color: #888;
  }
  .add-chip {
    font: inherit;
    font-size: 0.75rem;
    padding: 0.2rem 0.6rem;
    border: 1px solid #bbb;
    border-radius: 12px;
    background: #f4f4f4;
    color: #333;
    cursor: pointer;
  }
  .add-chip:hover {
    background: #e6e6e6;
  }
  .add-chip.rack {
    border-style: dashed;
    color: #666;
  }
  .world-hint {
    font-size: 0.8rem;
    color: #777;
    margin: 0.5rem 0;
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
  /* Small chrome buttons in a world item's top bar (rack collapse, device pull-out / flip). */
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
    border: 2px solid #4a4d52;
    border-radius: 6px;
    background: #1b1d20;
    padding: 14px; /* = FRAME_MARGIN, so guide rows align with mounted gear */
    position: relative;
  }
  .rack-label {
    position: absolute;
    top: 2px;
    left: 6px;
    font-size: 8px;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: #777;
  }
  .slots {
    display: flex;
    flex-direction: column-reverse; /* slot 0 at the bottom, matching uSlot indexing */
    height: 100%;
  }
  .slot {
    flex: 1;
    border-bottom: 1px dashed #3a3d42;
  }
  .slot:first-child {
    border-bottom: none;
  }
  kbd {
    background: #f0f0f0;
    border: 1px solid #ccc;
    border-radius: 3px;
    padding: 0.05em 0.35em;
  }
</style>
