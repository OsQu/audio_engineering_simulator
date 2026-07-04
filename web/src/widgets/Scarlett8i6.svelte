<script lang="ts">
  // A simplified Focusrite Scarlett 8i6 faceplate — the proving device for the per-device UI system
  // (Story 5.7). It composes the shared bound widgets (Control/Socket) inside Chassis with its own scoped
  // CSS, and lays its I/O across BOTH faces: the **front** carries the two combo inputs with their gain
  // knobs, per-channel INST/AIR/PAD indicator LEDs, and the monitor + headphone controls + headphone
  // jack; the **back** carries the line out, the USB send/return, MIDI, and the single power switch (a
  // Rust param group over every stage's `powered`). The signature red chassis comes from the skin
  // `accent` (Chassis outline + top-view tile). Faces are just which snippet a jack is written in — no
  // per-port face resolver. INST/AIR/PAD are **software-controlled** on the real 2nd-gen 8i6 (Focusrite
  // Control): the front panel shows only indicator LEDs — the actual toggles live in the device's focus
  // surface (FocusriteControl.svelte). 48V is still omitted (phantom not modeled). Exposed ids come from
  // the Rust `scarlett_8i6` entry: 0 Gain1 · 1 Pad1 · 2 Air1 · 3 Gain2 · 4 Pad2 · 5 Air2 · 6 Monitor ·
  // 7 Phones · 8 Power; Pad/Air are placed by the focus surface, so the guardrail unions both surfaces.
  import { untrack } from "svelte";
  import type { DeviceUiProps } from "../device-ui";
  import { makeHandle } from "../device-handle";
  import { skinFor } from "../skin";
  import Chassis from "./Chassis.svelte";
  import Control from "./Control.svelte";
  import Led from "./Led.svelte";
  import Legend from "./Legend.svelte";
  import Socket from "./Socket.svelte";

  let props: DeviceUiProps = $props();
  const skin = $derived(skinFor(props.typeId));
  // makeHandle reads the (stable) props object lazily, so capture it once; untrack documents that.
  const handle = makeHandle(untrack(() => props));
</script>

<Chassis {handle} flipped={props.flipped} finish={skin.finish} accent={skin.accent}>
  {#snippet front()}
    <div class="front">
      <!-- Two combo channels: gain knob, INST/AIR/PAD indicator LEDs (lit from config/param state;
           toggled in Focusrite Control), then the input jack. Written out per channel with **literal**
           ids so the faceplate guardrail can confirm each control/jack is placed and reachable. -->
      <div class="channel">
        <Control id={0} cap="dark" />
        <Legend text="Gain 1" />
        <div class="leds">
          <Led on={handle.config("inst1") >= 0.5} label="Inst" />
          <Led on={handle.value(2) >= 0.5} label="Air" />
          <Led on={handle.value(1) >= 0.5} label="Pad" />
        </div>
        <Socket dir="input" id={0} />
      </div>
      <div class="channel">
        <Control id={3} cap="dark" />
        <Legend text="Gain 2" />
        <div class="leds">
          <Led on={handle.config("inst2") >= 0.5} label="Inst" />
          <Led on={handle.value(5) >= 0.5} label="Air" />
          <Led on={handle.value(4) >= 0.5} label="Pad" />
        </div>
        <Socket dir="input" id={1} />
      </div>

      <!-- Monitor: the big centre knob. -->
      <div class="section monitor">
        <Legend text="Monitor" />
        <div class="big"><Control id={6} cap="dark" /></div>
      </div>

      <!-- Headphones: level + the front headphone jack. -->
      <div class="section">
        <Legend text="◎ Phones" />
        <Control id={7} cap="dark" />
        <Socket dir="output" id={3} />
      </div>

      <span class="wordmark">Scarlett <b>8i6</b></span>
    </div>
  {/snippet}

  {#snippet back()}
    <div class="back">
      <div class="section">
        <Legend text="Line Out" />
        <Socket dir="output" id={2} />
      </div>
      <div class="section">
        <Legend text="USB" />
        <div class="row">
          <Socket dir="output" id={0} />
          <Socket dir="output" id={1} />
          <Socket dir="input" id={2} />
        </div>
      </div>
      <div class="section">
        <Legend text="MIDI" />
        <div class="row">
          <Socket dir="input" id={3} />
          <Socket dir="output" id={4} />
        </div>
      </div>
      <div class="section">
        <Legend text="Power" />
        <!-- One switch for the whole unit — a real 8i6 is a single powered device (Rust param group). -->
        <Control id={8} />
      </div>
    </div>
  {/snippet}
</Chassis>

<style>
  /* Front: a horizontal strip of sections (two channels, monitor, headphones), the Focusrite idiom.
     Sizes are relative + container-query based so the strip scales with the chassis (small in the world,
     large in the focus overlay). */
  .front,
  .back {
    display: flex;
    flex-direction: row;
    align-items: center;
    justify-content: space-around;
    gap: clamp(4px, 3cqw, 1.2rem);
    height: 100%;
    box-sizing: border-box;
  }
  .channel,
  .section {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: clamp(1px, 2cqh, 0.3rem);
    min-width: 0;
  }
  .row {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: clamp(2px, 1.5cqw, 0.5rem);
  }
  /* The per-channel INST/AIR/PAD indicator lamps, in a tight row under the gain legend. */
  .leds {
    display: flex;
    flex-direction: row;
    gap: clamp(1px, 1cqw, 0.3rem);
  }
  /* The monitor knob reads as the hero control. */
  .big {
    transform: scale(1.35);
    transform-origin: center;
    padding: clamp(2px, 3cqh, 0.4rem);
  }
  .monitor {
    /* A hairline frame to set the monitor section apart, in the brand accent. */
    border: 1px solid color-mix(in srgb, var(--ae-accent, var(--ae-line-panel)) 45%, transparent);
    border-radius: var(--ae-radius-control);
    padding: clamp(2px, 2cqh, 0.35rem) clamp(3px, 2cqw, 0.6rem);
  }
  /* Brand wordmark, pinned bottom-left; "8i6" in the accent red. */
  .wordmark {
    position: absolute;
    left: clamp(3px, 2cqw, 0.7rem);
    bottom: clamp(2px, 3cqh, 0.5rem);
    font-family: var(--ae-font-display);
    font-weight: 700;
    font-size: clamp(5px, 16cqh, 0.7rem);
    letter-spacing: var(--ae-legend-spacing);
    color: var(--ae-faceplate-ink, var(--ae-text-primary));
    pointer-events: none;
  }
  .wordmark b {
    color: var(--ae-accent, var(--ae-text-primary));
  }
</style>
