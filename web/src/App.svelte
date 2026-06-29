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

  let status = $state("idle");
  let health = $state("");
  let midiStatus = $state("MIDI: requesting access…");
  let started = $state(false);
  let ready = $state(false);
  let catalog = $state<DeviceDescriptor[]>([]);
  let send = $state<((msg: ControlMessage) => void) | null>(null);

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
  const synthDesc = $derived(synthDevice ? descriptorFor(catalog, synthDevice.typeId) : undefined);

  const key = (device: string, paramId: number): string => `${device}:${paramId}`;

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

  function formatValue(p: ParamDescriptor, value: number): string {
    if (p.kind === "switch") return value >= 0.5 ? "on" : "off";
    const text = Number.isInteger(value) ? String(value) : value.toFixed(2);
    return p.unit ? `${text} ${p.unit}` : text;
  }

  async function start(): Promise<void> {
    if (started) return;
    started = true;
    try {
      await startEngine(plainPatch(), {
        onStatus: (m) => {
          status = m;
        },
        onHealth: (h) => {
          health = healthSummary(h);
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
      });
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
      <p>
        Play with the keyboard: <kbd>A</kbd> <kbd>W</kbd> <kbd>S</kbd> <kbd>E</kbd> <kbd>D</kbd>
        <kbd>F</kbd> <kbd>T</kbd> <kbd>G</kbd> <kbd>Y</kbd> <kbd>H</kbd> <kbd>U</kbd> <kbd>J</kbd>
        <kbd>K</kbd> map to one octave from C4. (<kbd>Z</kbd>/<kbd>X</kbd> shift octave down/up.)
      </p>

      {#if synthDevice && synthDesc}
        <h2>{synthDesc.name}</h2>
        {#each synthDesc.params as p (p.id)}
          {@const value = paramValues[key(synthDevice.id, p.id)] ?? p.default}
          <div class="control">
            <label for={`p-${p.id}`}>{p.label}</label>
            {#if p.kind === "switch"}
              <input
                id={`p-${p.id}`}
                type="checkbox"
                checked={value >= 0.5}
                oninput={(e) => onParamInput(synthDevice.id, p, e.currentTarget.checked ? 1 : 0)}
              />
            {:else}
              <input
                id={`p-${p.id}`}
                type="range"
                min={p.min}
                max={p.max}
                step={(p.max - p.min) / 200 || 0.01}
                {value}
                oninput={(e) => onParamInput(synthDevice.id, p, Number(e.currentTarget.value))}
              />
            {/if}
            <output>{formatValue(p, value)}</output>
          </div>
        {/each}
      {/if}

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
    max-width: 40rem;
    margin: 3rem auto;
    padding: 0 1rem;
    color: #1a1a1a;
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
  .control {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    margin: 0.4rem 0;
  }
  .control label {
    width: 7rem;
    flex: none;
  }
  .control input[type="range"] {
    flex: 1;
  }
  .control output {
    width: 5rem;
    flex: none;
    font-variant-numeric: tabular-nums;
    color: #555;
  }
  kbd {
    background: #f0f0f0;
    border: 1px solid #ccc;
    border-radius: 3px;
    padding: 0.05em 0.35em;
  }
</style>
