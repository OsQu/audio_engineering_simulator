<script lang="ts">
  // The harness shell, now in Svelte 5. It owns the authoritative scene and the reactive UI state;
  // the engine/worklet bring-up and control transport live in engine.ts. Controls are rendered
  // **from the fetched device catalog** (not hardcoded ids) — a generic stepping stone; the
  // skeuomorphic panel widgets land in Story 4.2.3. Generic by device id throughout.

  import type { CableType, DeviceDescriptor, ParamDescriptor, PortDomain, PortKind } from "./catalog";
  import { configDefault, descriptorFor, isPlayable } from "./catalog";
  import { deviceUi, focusUi } from "./device-ui";
  import { isFocusable } from "./focus";
  import { skinFor } from "./skin";
  import {
    type ControlMessage,
    healthSummary,
    type ReadyMessage,
    startEngine,
    wireKeyboard,
    wireMidi,
  } from "./engine";
  import { cablePathData, cableTypeIdFor } from "./connections";
  import type { ConnectVerdict } from "./connections";
  import type { Connection, Patch, PortRef } from "./scene";
  import { DEFAULT_VELOCITY } from "./notes";
  import {
    defaultScene,
    loadScene,
    type Scene,
    saveScene,
    setSceneConfig,
    setSceneParam,
  } from "./scene-store";
  import {
    deviceById,
    effectiveFacing,
    GRID_MM,
    isRack,
    type LayoutCtx,
    type PlacedItem,
    placedItemsFor,
    rackById,
    type ViewCtx,
  } from "./projection";
  import * as cableView from "./cable-view";
  import { measureJacks as measureJacksDom } from "./jack-anchors";
  import * as params from "./params";
  import * as patching from "./patching";
  import type { JackHit, PatchState } from "./patching";
  import * as placement from "./placement";
  import * as sceneOps from "./scene-ops";
  import { type Room, type Wall } from "./spatial";
  import Keybed from "./widgets/Keybed.svelte";
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

  // Collapse the toolbar `<details>` menu a control lives in, so picking an action closes its drawer.
  function closeMenu(e: Event): void {
    (e.currentTarget as HTMLElement | null)?.closest("details")?.removeAttribute("open");
  }

  // The page's authoritative scene: a saved one if present, else the default studio. The plain
  // `initialScene` const lets both `scene` and `currentSpace` seed from the same value without one
  // $state initializer reading another (which would only capture its initial value).
  const initialScene = loadScene() ?? defaultScene();
  let scene = $state<Scene>(initialScene);

  // Live control-param values, keyed `device:paramId`, mirrored into the scene on change so they
  // persist on save. Re-seeded from the scene whenever it's (re)loaded.
  let paramValues = $state<Record<string, number>>({});

  const key = params.key;

  // The current value of a device-local param (live override else descriptor default), bound to the map.
  const paramValue = (deviceId: string, desc: DeviceDescriptor, id: number): number =>
    params.paramValue(paramValues, deviceId, desc, id);

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

  // Human labels for the wall-view switcher.
  const WALL_LABELS: Record<Wall, string> = { front: "Front", back: "Back", left: "Left", right: "Right" };

  // The projection context, rebuilt inline on each call so its field reads register as reactive
  // dependencies of whatever $derived/handler invokes it. Never hoist these into a plain const at
  // module-init time — the fields would be captured stale.
  const view = (): ViewCtx => ({ space: currentSpace, view: currentView, wall: currentWall, room });
  const layout = (): LayoutCtx => ({ ...view(), scene, catalog });

  const placedItems = $derived.by((): PlacedItem[] => placedItemsFor(layout()));

  // --- Patch cables --------------------------------------------------------------------------------
  // A stable key for a connection (its two endpoints), for the {#each} in the cable overlay.
  const connKey = sceneOps.connKey;

  // Measured jack-connector centres in surface space, keyed "device:direction:port" (from each jack's
  // `data-jack` attribute), each tagged with the chassis face it sits on. Populated by measureJacks after
  // layout; lets a cable anchor at the real socket when its jack is on the shown face, falling back to the
  // chassis edge otherwise.
  let jackAnchors = $state<Record<string, cableView.JackAnchor>>({});

  // Re-measure jack anchors into surface space (the DOM work lives in jack-anchors.ts); the $effect
  // below schedules it on layout changes.
  const measureJacks = (): void => {
    if (worldApi) jackAnchors = measureJacksDom(worldApi);
  };

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
    JSON.stringify(scene.ui.racks); // a rack flip changes which jacks (front/back) are shown
    JSON.stringify(scene.patch.connections);
    const raf = requestAnimationFrame(measureJacks);
    const settle = setTimeout(measureJacks, 480);
    return () => {
      cancelAnimationFrame(raf);
      clearTimeout(settle);
    };
  });

  // Cable end anchor + view-membership helpers are pure cable-view fns, prebound here to the layout ctx,
  // the measured jack anchors, and (for cableAnchor) the world api.
  const cableAnchor = (
    ref: PortRef,
    direction: "input" | "output",
    api: WorldApi,
  ): { x: number; y: number } | null => cableView.cableAnchor(layout(), jackAnchors, ref, direction, api);

  // Cable occlusion is handled by z-order, not by the cable: a single continuous lead is drawn in the
  // cable layer (z 2), and each device sits above or below it by facing (see placedItems' `z`) — a
  // back-facing device (visible sockets) below, a front-facing one above. So a cable plugs into a visible
  // socket yet tucks behind a hidden panel, with no split. (A faceplate may place jacks on either face;
  // cableAnchor anchors precisely only at jacks on the shown face — see its face-match rule.)

  const inView = (id: string): boolean => cableView.inView(layout(), id);
  const bothInView = (c: Connection): boolean => cableView.bothInView(layout(), c);
  const oneInView = (c: Connection): boolean => cableView.oneInView(layout(), c);
  const otherEndLabel = (id: string): string => cableView.otherEndLabel(layout(), WALL_LABELS, id);
  const portalKey = cableView.portalKey;
  const portalOffset = (c: Connection, fromIn: boolean): { dx: number; dy: number } =>
    cableView.portalOffset(scene, c, fromIn);

  // Dragging a portal chip: its key + the (fixed) jack anchor it hangs off + the world converters. The
  // offset is recomputed live as cursor − anchor and stored in the scene, so it persists on save.
  let portalDrag: { key: string; anchor: { x: number; y: number }; api: WorldApi } | null = null;
  function startPortalDrag(e: PointerEvent, key: string, anchor: { x: number; y: number }, api: WorldApi): void {
    e.preventDefault();
    e.stopPropagation(); // don't let the window cable/pan handlers see this press
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    portalDrag = { key, anchor, api };
  }
  function onPortalDragMove(e: PointerEvent): void {
    if (!portalDrag) return;
    const s = portalDrag.api.clientToSurface(e.clientX, e.clientY);
    scene.ui.portals ??= {};
    scene.ui.portals[portalDrag.key] = { dx: s.x - portalDrag.anchor.x, dy: s.y - portalDrag.anchor.y };
  }
  function onPortalDragEnd(e: PointerEvent): void {
    if (!portalDrag) return;
    (e.currentTarget as Element).releasePointerCapture?.(e.pointerId);
    portalDrag = null;
  }

  // --- Patching (drag or click-to-pick) -------------------------------------------------------------
  // An in-progress patch from a source jack: the source end + its anchor, the moving free end (surface
  // coords), and — when hovering a candidate jack — the verdict so we can colour the cable + commit.
  // `mode` is "drag" while the pointer is held (same-view patching), or "pending" after a *click* — a
  // held cable that survives a wall/room switch, so you can complete a **cross-view** patch by clicking
  // the source jack, turning to the other wall/room, and clicking the destination jack.
  let dragCable = $state<PatchState>(null);
  // Pointer-down bookkeeping to tell a click (→ pending pick) from a drag (moved past a small threshold).
  let cableDown = { x: 0, y: 0, moved: false };

  // The display name of the pending patch's source device (for the "patching from…" banner).
  const pendingSourceName = $derived.by((): string | null => {
    if (dragCable?.mode !== "pending") return null;
    const dev = deviceById(scene, dragCable.source.device);
    return (dev && descriptorFor(catalog, dev.typeId)?.name) || dragCable.source.device;
  });

  // Resolve a `data-jack` key into the JackHit the pure transitions need (endpoint + measured anchor),
  // or null if it doesn't name a real, resolvable port.
  const jackHitOf = (key: string | undefined | null): JackHit | null => {
    if (!key) return null;
    const endpoint = patching.endpointFromJackKey(scene, catalog, key);
    return endpoint ? { key, endpoint, anchor: jackAnchors[key] ?? null } : null;
  };
  const patchDeps = () => ({ connections: scene.patch.connections });

  // The patching handlers are thin adapters: they read the DOM into a JackHit + surface coords, keep the
  // click-vs-drag threshold bookkeeping, call the pure transition, assign its state, and on a returned
  // `commit` verdict commit the cable (which hot-swaps). See patching.ts for the transition semantics.
  function onCablePointerDown(e: PointerEvent): void {
    if (!worldApi) return;
    // A pending cable's second press: only record the point; onCablePointerUp resolves click-vs-pan.
    if (dragCable?.mode === "pending") {
      cableDown = { x: e.clientX, y: e.clientY, moved: false };
      return;
    }
    const hit = jackHitOf((e.target as HTMLElement | null)?.closest<HTMLElement>("[data-jack]")?.dataset.jack);
    if (!hit || !hit.anchor) return; // only a measured jack can start a drag
    e.preventDefault();
    cableDown = { x: e.clientX, y: e.clientY, moved: false };
    dragCable = patching.pointerDown(dragCable, hit).state;
  }

  function onCablePointerMove(e: PointerEvent): void {
    if (!dragCable || !worldApi) return;
    // Track whether the active press has moved past the click threshold — but only while a button is
    // held (`e.buttons`), so a pending cable's buttonless cursor-follow is never mistaken for a pan.
    if (e.buttons !== 0 && !cableDown.moved) {
      if (Math.hypot(e.clientX - cableDown.x, e.clientY - cableDown.y) > 4) cableDown.moved = true;
    }
    const cursor = worldApi.clientToSurface(e.clientX, e.clientY);
    const srcAnchor = jackAnchors[patching.jackKeyOf(dragCable.source)] ?? null;
    const hit = jackHitOf(
      document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>("[data-jack]")?.dataset.jack,
    );
    dragCable = patching.pointerMove(dragCable, hit, cursor, srcAnchor, patchDeps());
  }

  function onCablePointerUp(e: PointerEvent): void {
    if (!dragCable) return;
    // The pending second-click needs the jack under the release point; drag-release uses the last verdict.
    const hit =
      dragCable.mode === "pending" && !cableDown.moved
        ? jackHitOf(document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>("[data-jack]")?.dataset.jack)
        : null;
    const res = patching.pointerUp(dragCable, hit, !cableDown.moved, patchDeps());
    if (res.commit) commitCable(res.commit);
    dragCable = res.state;
  }

  // Esc closes the focus overlay first, else cancels an in-progress patch (drag or pending).
  function onGlobalKey(e: KeyboardEvent): void {
    if (e.key !== "Escape") return;
    if (focusedDevice !== null) {
      focusedDevice = null;
      return;
    }
    if (dragCable) dragCable = patching.cancel();
  }

  // Connection introspection + edits are pure scene-ops; App wraps them to bind scene/catalog/cables
  // and to hot-swap the engine after any edit that changes the runnable patch.
  const connectionDomain = (c: Connection): PortDomain | null =>
    sceneOps.connectionDomain(scene, catalog, c);
  const connectionKind = (c: Connection): PortKind => sceneOps.connectionKind(scene, catalog, c);

  function commitCable(v: ConnectVerdict): void {
    sceneOps.commitCable(scene, catalog, cables, v);
    hotSwap();
  }

  // Remove a cable (from the inspector) and hot-swap; drop the inspector selection if it was this one.
  function disconnect(c: Connection): void {
    const k = connKey(c);
    sceneOps.disconnect(scene, c);
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
  // The cable presets that physically fit the selected connection (matching connector) — the picker
  // offers only these, so you can't put an XLR cable on a ¼" link.
  const cablesForSelected = $derived.by((): CableType[] =>
    selectedConn ? sceneOps.cablesFor(scene, catalog, cables, selectedConn) : [],
  );

  // --- Device focus mode (sit down at a device: a large, device-specific interaction surface) --------
  // Transient UI state (like the cable-inspector selection) — never persisted to scene `ui`. The
  // presentation is an overlay that dims the world (a peer of the cable-inspector / patch-banner
  // overlays), not a WorldView spatial change.
  let focusedDevice = $state<string | null>(null);
  // The focused device resolved to its instance + descriptor — or null when nothing is focused (or the
  // focused device has gone / isn't focusable, so a stale id renders nothing). Which surface component
  // to draw is `focusUi(typeId)`; whether a keybed is appended is `isPlayable(desc)`.
  const focused = $derived.by(() => {
    if (focusedDevice === null) return null;
    const device = deviceById(scene, focusedDevice);
    const desc = device ? descriptorFor(catalog, device.typeId) : undefined;
    if (!device || !desc || !isFocusable(desc)) return null;
    return { device, desc };
  });
  const closeFocus = (): void => {
    focusedDevice = null;
  };
  // The focus dialog element, for a basic focus-trap: move keyboard focus into the surface when it
  // opens (depends only on the focused *id* + the element, so turning a knob doesn't steal focus back).
  let focusSurfaceEl = $state<HTMLElement | undefined>();
  $effect(() => {
    if (focusedDevice !== null) focusSurfaceEl?.focus();
  });

  // --- Playing the focused instrument (note input follows focus) ------------------------------------
  // Whether a device's events input is fed by a cable (an incoming connection to its events-in port).
  // If so, host-injected notes are a no-op — the performance comes from the patched source instead — so
  // the on-screen keybed shows disabled and the keyboard doesn't target it.
  function eventsInputDriven(deviceId: string, desc: DeviceDescriptor): boolean {
    const evPort = desc.ports.find((p) => p.direction === "input" && p.domain === "events");
    if (!evPort) return false;
    return scene.patch.connections.some((c) => c.to.device === deviceId && c.to.port === evPort.id);
  }
  // The device a keyboard/MIDI/on-screen note plays: the focused device iff it's an instrument (a
  // keybed surface) whose events input is *open* (not cable-driven), else null. A plain string|null so
  // the wireKeyboard effect below only re-runs when the *target* changes — turning a knob (which
  // mutates the scene) doesn't re-attach the listener.
  const keyboardTarget = $derived.by((): string | null => {
    if (focusedDevice === null) return null;
    const dev = deviceById(scene, focusedDevice);
    const desc = dev ? descriptorFor(catalog, dev.typeId) : undefined;
    if (!dev || !desc || !isPlayable(desc)) return null;
    return eventsInputDriven(dev.id, desc) ? null : dev.id;
  });
  // Notes currently sounding, for the keybed highlight — fed by every source (mouse, QWERTY, MIDI) so
  // the on-screen keys light up whichever way you play.
  let heldNotes = $state<number[]>([]);
  // Route one note-on/off to the focused instrument: update the held set (for the highlight) and post
  // it to the worklet. A no-op when nothing playable is focused or the engine isn't up yet.
  function playNote(on: boolean, note: number, velocity: number = DEFAULT_VELOCITY): void {
    const device = keyboardTarget;
    if (device === null || !send) return;
    if (on) {
      if (!heldNotes.includes(note)) heldNotes = [...heldNotes, note];
      send({ type: "noteOn", device, note, velocity });
    } else {
      heldNotes = heldNotes.filter((n) => n !== note);
      send({ type: "noteOff", device, note });
    }
  }
  // Capture the computer keyboard **only while an instrument is focused** (attach on focus, detach on
  // unfocus — the effect re-runs just when keyboardTarget changes). Web MIDI is wired once at start-up
  // (below); both feed playNote, which targets the focused instrument.
  $effect(() => {
    if (keyboardTarget === null) return;
    return wireKeyboard(playNote);
  });

  // Set (or clear, `""` ⇒ ideal wire) the cable type on a connection, then hot-swap — the cable's R·C
  // is baked into the edge at compile, so changing it rebuilds the engine.
  function setCableType(c: Connection, typeId: string): void {
    sceneOps.setCableType(scene, cables, c, typeId);
    hotSwap();
  }

  // Thin adapters over placement.ts, handed to the world layer as the drag legality + commit hooks.
  // Both build the layout ctx inline (so reads stay reactive) and pass the already-derived placedItems.
  const canPlace = (id: string, x: number, y: number): boolean =>
    placement.canPlace(layout(), placedItems, id, x, y);
  const moveTo = (id: string, x: number, y: number): void => placement.moveTo(layout(), id, x, y);

  // Spaces (rooms) + cross-space moves + flip — UI-only scene furniture (no hot-swap). Thin wrappers
  // over scene-ops; addSpace returns the new id so we switch to it.
  const addSpace = (): void => {
    currentSpace = sceneOps.addSpace(scene);
  };
  const moveDeviceToSpace = (id: string, spaceId: string): void =>
    sceneOps.moveDeviceToSpace(scene, id, spaceId);
  const moveRackToSpace = (id: string, spaceId: string): void =>
    sceneOps.moveRackToSpace(scene, id, spaceId);
  const toggleFlip = (id: string): void => sceneOps.toggleFlip(scene, id);
  const toggleRackFlip = (id: string): void => sceneOps.toggleRackFlip(scene, id);
  const unmount = (id: string): void => sceneOps.unmount(scene, id);

  // A knob move touches all three param lanes at once — the live map (UI), the scene (for save), and the
  // engine (live) — so keep them in sync in this one visible place; they mustn't drift apart.
  function onParamInput(device: string, p: ParamDescriptor, value: number): void {
    paramValues[key(device, p.id)] = value;
    setSceneParam(scene, device, p.id, value);
    send?.({ type: "param", device, paramId: p.id, value });
  }

  // A structural config's current value in the scene, falling back to the descriptor's build default —
  // the mirror of `paramValue` for the (recompile-on-change) config lane.
  const configValue = (deviceId: string, desc: DeviceDescriptor, key: string): number => {
    const set = deviceById(scene, deviceId)?.config?.find((c) => c.key === key);
    return set?.value ?? configDefault(desc, key);
  };

  // A structural config toggle (INST/hi-Z): unlike a knob, this changes how the device is *built*, so it
  // edits the scene and rebuilds the engine (the same hot-swap repatching uses) rather than a live param.
  function onConfigInput(device: string, key: string, value: number): void {
    setSceneConfig(scene, device, key, value);
    hotSwap();
  }

  // A structural edit → rebuild the engine from the new patch (compile + ScheduleSlot hot-swap, in the
  // worklet, the Story 4.1 path) and re-apply param values. Edits are rare gestures, so the off-block
  // compile cost is acceptable; the live audio thread swaps at a block boundary.
  function hotSwap(): void {
    if (!send) return;
    send({ type: "loadPatch", patch: plainPatch() });
    paramValues = params.seedParamValues(scene, catalog);
    params.pushParams(send, scene, catalog, paramValues);
  }

  // Add gear (rebuilds the engine) / a rack (UI furniture, no rebuild); remove either. Thin wrappers
  // over scene-ops — addDevice/removeDevice hot-swap, the rack ops don't (per the plan's table).
  function addDevice(typeId: string): void {
    sceneOps.addDevice(layout(), placedItems, typeId);
    hotSwap();
  }
  function removeDevice(id: string): void {
    if (focusedDevice === id) focusedDevice = null;
    sceneOps.removeDevice(scene, id);
    hotSwap();
  }
  const addRack = (): void => sceneOps.addRack(layout(), placedItems);
  const removeRack = (id: string): void => sceneOps.removeRack(scene, id);

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
          paramValues = params.seedParamValues(scene, catalog);
          params.pushParams(sendFn, scene, catalog, paramValues); // match the engine to the scene from the start
          // Request Web MIDI once (the permission); the note target follows focus via playNote. The
          // computer keyboard is wired per-focus by the effect above, not here.
          wireMidi((on, note, velocity) => playNote(on, note, velocity), (m) => {
            midiStatus = m;
          });
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
    paramValues = params.seedParamValues(scene, catalog);
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
  onkeydown={onGlobalKey}
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

      <!-- View switcher: turn to face each wall of the room, or look down on the top-down floor plan. -->
      <div class="views" role="group" aria-label="view">
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
        <button
          type="button"
          class="view-tab top"
          class:active={currentView === "top"}
          onclick={() => (currentView = "top")}
        >
          Top
        </button>
      </div>

      <!-- Gear catalog, tucked in a drawer so it isn't always spilling across the toolbar. -->
      <details class="menu">
        <summary>+ Add</summary>
        <div class="menu-panel palette">
          {#each catalog as desc (desc.typeId)}
            <button
              type="button"
              class="add-chip"
              onclick={(e) => {
                addDevice(desc.typeId);
                closeMenu(e);
              }}
            >
              {desc.name}
            </button>
          {/each}
          <button
            type="button"
            class="add-chip rack"
            onclick={(e) => {
              addRack();
              closeMenu(e);
            }}
          >
            Rack
          </button>
        </div>
      </details>

      <!-- Monitor volume + scene save/load/reload, behind a menu. -->
      <details class="menu right push">
        <summary>⚙ Scene</summary>
        <div class="menu-panel scene-menu">
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
          <span class="scene-buttons">
            <button type="button" onclick={(e) => { saveCurrent(); closeMenu(e); }}>save</button>
            <button type="button" onclick={(e) => { loadSaved(); closeMenu(e); }}>load</button>
            <button type="button" onclick={(e) => { reload(); closeMenu(e); }}>reload</button>
          </span>
        </div>
      </details>

      <!-- Global VU + simulation status/health/MIDI readout, behind a debug menu. -->
      <details class="menu right">
        <summary>Debug</summary>
        <div class="menu-panel debug-menu">
          <Vu {level} />
          <span class="statuses">{[status, health, midiStatus].filter(Boolean).join(" · ")}</span>
        </div>
      </details>
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
          {@const off = portalOffset(c, fromIn)}
          {@const p = { x: a.x + off.dx, y: a.y + off.dy }}
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
          <!-- The dot is a drag handle: drag it to move the portal chip out of the way (offset persists). -->
          <circle
            class="portal-dot"
            cx={p.x}
            cy={p.y}
            r="16"
            role="button"
            tabindex="-1"
            aria-label={`move portal ${connKey(c)}`}
            onpointerdown={(e) => startPortalDrag(e, portalKey(c, fromIn), a, api)}
            onpointermove={onPortalDragMove}
            onpointerup={onPortalDragEnd}
          ></circle>
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
          <!-- Top-down floor plan: the room outline + which wall is which edge (front = far, back = near). -->
          {#if currentView === "top"}
            {@const bl = api.worldToSurface(0, 0)}
            {@const tr = api.worldToSurface(room.width, room.depth)}
            <g class="room-plan">
              <rect x={bl.x} y={tr.y} width={tr.x - bl.x} height={bl.y - tr.y} />
              <text class="plan-wall" x={(bl.x + tr.x) / 2} y={tr.y + 40} text-anchor="middle">Front</text>
              <text class="plan-wall" x={(bl.x + tr.x) / 2} y={bl.y - 18} text-anchor="middle">Back</text>
              <text class="plan-wall" x={bl.x + 24} y={(bl.y + tr.y) / 2} text-anchor="start">Left</text>
              <text class="plan-wall" x={tr.x - 24} y={(bl.y + tr.y) / 2} text-anchor="end">Right</text>
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
          {#if isRack(scene, itemId)}
            {@const rack = rackById(scene, itemId)}
            {#if rack}
              {#if currentView !== "top"}
                <!-- Turn the whole rack around to reach the rear I/O of all its mounted gear at once
                     (no panel is shown in the top-down plan, so the flip is hidden there). -->
                <button type="button" class="chip" aria-label="turn rack around" onclick={() => toggleRackFlip(itemId)}>
                  {rack.facing === "back" ? "front" : "back"}
                </button>
              {/if}
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
            {@const focusDesc = descriptorFor(catalog, deviceById(scene, itemId)?.typeId ?? "")}
            {#if place}
              {#if currentView !== "top"}
                <!-- Sit down at a device that warrants deep control (a synth keybed, a console): open its
                     large focus surface. Only devices with a surface show the chip (converters/speaker
                     don't). A wall-elevation affordance, like flip — no panel is shown in the top plan. -->
                {#if focusDesc && isFocusable(focusDesc)}
                  <button type="button" class="chip" aria-label="open {focusDesc.name}" onclick={() => (focusedDevice = itemId)}>
                    open
                  </button>
                {/if}
                <!-- Flip/eject are wall-elevation affordances (no panel is shown in the top-down plan).
                     A bolted-in unit can't be flipped on its own — turn its rack around instead, or
                     eject it here to flip it free-standing. -->
                {#if place.rack}
                  <button type="button" class="chip" aria-label="eject from rack" onclick={() => unmount(itemId)}>
                    eject
                  </button>
                {:else}
                  <button type="button" class="chip" onclick={() => toggleFlip(itemId)}>
                    {place.facing === "back" ? "front" : "back"}
                  </button>
                {/if}
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
          {#if currentView === "top"}
            <!-- Top-down plan: a labelled floor footprint (no panel — you can't see a face from above). -->
            {@const rack = rackById(scene, itemId)}
            {@const device = deviceById(scene, itemId)}
            {@const desc = device ? descriptorFor(catalog, device.typeId) : undefined}
            <!-- A device's brand accent (the skin's chassis colour) rims its floor tile, so a red
                 Focusrite reads as red from directly above too — one value, both views. -->
            {@const accent = device ? skinFor(device.typeId).accent : undefined}
            <div class="plan-tile" class:rack={!!rack} style:--tile-accent={accent}>
              <span>{rack ? `${rack.id} · ${rack.slots}U` : (desc?.name ?? itemId)}</span>
            </div>
          {:else if isRack(scene, itemId)}
            {@const rack = rackById(scene, itemId)}
            {#if rack}
              <div class="rack-frame" class:rear={rack.facing === "back"}>
                <span class="rack-label">{rack.id} · {rack.slots}U{rack.facing === "back" ? " · rear" : ""}</span>
                <div class="slots">
                  {#each Array.from({ length: rack.slots }, (_, i) => i) as i (i)}
                    <div class="slot"></div>
                  {/each}
                </div>
              </div>
            {/if}
          {:else}
            {@const device = deviceById(scene, itemId)}
            {@const desc = device ? descriptorFor(catalog, device.typeId) : undefined}
            {@const place = scene.ui.placements[itemId]}
            {#if device && desc && place}
              <!-- The device's registered faceplate (its own component, or the generic Panel). -->
              {@const Faceplate = deviceUi(device.typeId)}
              <Faceplate
                device={device.id}
                typeId={device.typeId}
                name={desc.name}
                params={desc.params}
                ports={desc.ports}
                readouts={desc.readouts}
                configs={desc.configs}
                flipped={effectiveFacing(scene, device.id) === "back"}
                valueFor={(id) => paramValue(device.id, desc, id)}
                readingFor={(id) => readingFor(device.id, id)}
                onParam={(p, v) => onParamInput(device.id, p, v)}
                configFor={(k) => configValue(device.id, desc, k)}
                onConfig={(k, v) => onConfigInput(device.id, k, v)}
              />
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
                {#each cablesForSelected as ct (ct.typeId)}
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

      {#if focused}
        {@const f = focused}
        <!-- The device's focus surface (its dedicated one — e.g. a console — or its faceplate). -->
        {@const Surface = focusUi(f.device.typeId)}
        <!-- Device focus overlay: sit down at the device. Dims the world and shows its surface large — a
             peer of the cable-inspector / patch-banner overlays, not a WorldView spatial change. Click the
             backdrop or press Esc to leave. The surface reuses the same descriptor-driven Panel props as
             the in-world panel (Story 4.8.4 adds the instrument keybed, 4.8.6 the console). -->
        <div
          class="focus-backdrop"
          role="button"
          tabindex="-1"
          aria-label="close focus"
          onclick={(e) => {
            if (e.target === e.currentTarget) closeFocus();
          }}
          onkeydown={(e) => {
            if (e.key === "Enter") closeFocus();
          }}
        >
          <div
            class="focus-surface"
            bind:this={focusSurfaceEl}
            role="dialog"
            aria-modal="true"
            aria-label={`${f.desc.name} — focus`}
            tabindex="-1"
          >
            <header class="focus-head">
              <span class="focus-name">{f.desc.name}</span>
              <button type="button" class="focus-close" onclick={closeFocus}>Close</button>
            </header>
            <div class="focus-body">
              <Surface
                device={f.device.id}
                typeId={f.device.typeId}
                name={f.desc.name}
                params={f.desc.params}
                ports={f.desc.ports}
                readouts={f.desc.readouts}
                configs={f.desc.configs}
                valueFor={(id) => paramValue(f.device.id, f.desc, id)}
                readingFor={(id) => readingFor(f.device.id, id)}
                onParam={(p, v) => onParamInput(f.device.id, p, v)}
                configFor={(k) => configValue(f.device.id, f.desc, k)}
                onConfig={(k, v) => onConfigInput(f.device.id, k, v)}
              />
              {#if isPlayable(f.desc)}
                <!-- The keybed = the device's open events input, drawn on-screen. Disabled when the input
                     is cable-driven (a patched controller performs it instead — host notes are a no-op). -->
                <Keybed held={heldNotes} onNote={playNote} disabled={eventsInputDriven(f.device.id, f.desc)} />
              {/if}
            </div>
          </div>
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
    max-width: 22rem;
    font-size: 0.75rem;
    color: var(--ae-text-muted);
    font-variant-numeric: tabular-nums;
  }
  .stage {
    position: relative; /* anchor for the floating cable inspector */
    flex: 1;
    min-height: 0;
  }
  /* Toolbar dropdown: a <details> whose <summary> is a chip-styled button and whose panel floats below. */
  .menu {
    position: relative;
  }
  .menu.push {
    margin-left: auto; /* push this menu (and any after it) to the toolbar's right edge */
  }
  .menu > summary {
    list-style: none;
    cursor: pointer;
    user-select: none;
    padding: 0.5em 1.2em;
    font-size: 0.85rem;
    color: var(--ae-text-strong);
    background: var(--ae-bg-chip);
    border: 1px solid var(--ae-line-chip);
    border-radius: var(--ae-radius-control);
  }
  .menu > summary::-webkit-details-marker {
    display: none;
  }
  .menu[open] > summary,
  .menu > summary:hover {
    background: var(--ae-bg-panel);
  }
  .menu-panel {
    position: absolute;
    top: calc(100% + 0.35rem);
    z-index: 20;
    display: flex;
    gap: 0.5rem;
    padding: 0.6rem;
    background: var(--ae-bg-panel);
    border: 1px solid var(--ae-line-panel);
    border-radius: var(--ae-radius-control);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.35);
  }
  .menu.right .menu-panel {
    right: 0; /* right-anchored menus open flush to their right edge, not off-screen */
  }
  .menu-panel.scene-menu,
  .menu-panel.debug-menu {
    flex-direction: column;
    align-items: flex-start;
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
  /* The Top tab is set off from the four walls (a different kind of view). */
  .view-tab.top {
    border-left-width: 2px;
  }
  /* Top-down plan tile — a labelled floor footprint (no panel). */
  .plan-tile {
    width: 100%;
    height: 100%;
    box-sizing: border-box;
    display: flex;
    align-items: center;
    justify-content: center;
    text-align: center;
    padding: 2px;
    /* A device's brand accent rims the tile when its skin sets one, else the neutral edge. */
    border: 1px solid var(--tile-accent, var(--ae-line-hard));
    border-radius: 4px;
    background: var(--ae-bg-chip);
    color: var(--ae-text-strong);
    font-size: 11px;
    line-height: 1.1;
    overflow: hidden;
  }
  .plan-tile.rack {
    background: linear-gradient(var(--ae-rack-shell-1), var(--ae-rack-shell-2));
    color: var(--ae-text-muted);
    border-color: var(--ae-line-hard);
    letter-spacing: var(--ae-legend-spacing);
    text-transform: uppercase;
  }
  /* Room outline + wall labels drawn over the floor plan. */
  .room-plan rect {
    fill: none;
    stroke: var(--ae-line-panel);
    stroke-width: 3;
    stroke-dasharray: 10 8;
  }
  .plan-wall {
    fill: var(--ae-text-muted);
    font-size: 34px;
    letter-spacing: var(--ae-legend-spacing);
    text-transform: uppercase;
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
    flex-wrap: wrap;
    align-items: center;
    max-width: 24rem;
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
    /* The overlay is pointer-transparent by default; the dot opts back in so it can be grabbed + dragged. */
    pointer-events: all;
    cursor: grab;
  }
  .portal-dot:active {
    cursor: grabbing;
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
  /* Device focus overlay — dims the world and centres the focused device's surface. Above the toolbar
     menus (z 20) so nothing peeks through; covers the stage only (the toolbar stays reachable). */
  .focus-backdrop {
    position: absolute;
    inset: 0;
    z-index: 30;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 2rem;
    background: rgb(0 0 0 / 0.55);
    cursor: default;
  }
  .focus-surface {
    display: flex;
    flex-direction: column;
    gap: 0.8rem;
    max-width: min(90%, 900px);
    max-height: 90%;
    overflow: auto;
    padding: 1rem 1.2rem 1.4rem;
    background: var(--ae-bg-panel);
    border: 1px solid var(--ae-line-panel);
    border-radius: var(--ae-radius-panel);
    box-shadow: var(--ae-shadow-card);
  }
  .focus-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
  }
  .focus-name {
    font-size: 1rem;
    font-weight: 600;
    color: var(--ae-text-strong);
  }
  .focus-close {
    font: inherit;
    font-size: 0.72rem;
    padding: 0.2rem 0.7rem;
    color: var(--ae-text-strong);
    background: var(--ae-bg-chip);
    border: 1px solid var(--ae-line-chip);
    border-radius: var(--ae-radius-control);
    cursor: pointer;
  }
  /* Blow the panel up to fill the surface: the in-world panel is sized small for the world, but the focus
     view has room, so let it grow. */
  .focus-body {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1.2rem;
  }
  .focus-body :global(.panel) {
    width: 100%;
    min-height: 220px;
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
  /* Rear of the rack: a cooler, flatter shell so a turned-around rack reads as "the back" at a glance
     (the mounted gear renders its own back panels; this is just the cabinet cue). */
  .rack-frame.rear {
    background: linear-gradient(var(--ae-rack-shell-2), var(--ae-rack-shell-1));
    filter: brightness(0.9) saturate(0.8);
  }
</style>
