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
  import type { Patch } from "./scene";
  import { defaultScene, loadScene, type Scene, saveScene, setSceneParam } from "./scene-store";
  import Panel from "./widgets/Panel.svelte";
  import Screen from "./widgets/Screen.svelte";
  import Vu from "./widgets/Vu.svelte";

  let status = $state("idle");
  let health = $state("");
  let midiStatus = $state("MIDI: requesting access…");
  let started = $state(false);
  let ready = $state(false);
  let catalog = $state<DeviceDescriptor[]>([]);
  let send = $state<((msg: ControlMessage) => void) | null>(null);
  // Master output peak (linear, ±1.0 = full scale), from the worklet's throttled level message.
  let level = $state(0);

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

  // The page's authoritative scene: a saved one if present, else the default studio.
  let scene = $state<Scene>(loadScene() ?? defaultScene());

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
          // Push the seeded values so the engine matches the scene from the first interaction.
          for (const device of scene.patch.devices) {
            const desc = descriptorFor(catalog, device.typeId);
            if (!desc) continue;
            for (const p of desc.params) {
              sendFn({
                type: "param",
                device: device.id,
                paramId: p.id,
                value: paramValues[key(device.id, p.id)],
              });
            }
          }
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
    seedParamValues();
    send?.({ type: "loadPatch", patch: plainPatch() }); // hot-swap the engine to the saved scene
    status = "scene loaded";
  }

  function reload(): void {
    send?.({ type: "loadPatch", patch: plainPatch() }); // re-apply current scene — proves glitch-free swap
    status = "scene reloaded (hot-swap)";
  }
</script>

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

      <div class="rack">
        {#each scene.patch.devices as device (device.id)}
          {@const desc = descriptorFor(catalog, device.typeId)}
          {#if desc}
            <Panel
              name={desc.name}
              params={desc.params}
              ports={desc.ports}
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
        {/each}
      </div>

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
  .rack {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem;
    align-items: flex-start;
    margin: 0.5rem 0 1rem;
  }
  kbd {
    background: #f0f0f0;
    border: 1px solid #ccc;
    border-radius: 3px;
    padding: 0.05em 0.35em;
  }
</style>
