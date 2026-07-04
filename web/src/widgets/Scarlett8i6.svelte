<script lang="ts">
  // The full Focusrite Scarlett 8i6 faceplate (Story 5.7.8) — the proving device for the per-device UI
  // system. It composes the shared bound widgets (Control/Socket) inside Chassis and lays its I/O across
  // BOTH faces per the real 2nd-gen panels:
  //   • front — the two combo inputs with gain knobs + per-channel INST/AIR/PAD indicator LEDs, the big
  //     MONITOR hero knob, and the two headphone jacks each with its level knob;
  //   • back — the rear line inputs 3–6, line outputs 1–4, S/PDIF in/out, the USB and MIDI connectors, a
  //     decorative 12V-DC inlet silkscreen, and the single power switch (a Rust param group over every
  //     stage's `powered`).
  // The signature red chassis comes from the skin `accent`. Faces are just which snippet a jack is
  // written in — no per-port face resolver. INST/AIR/PAD are software-controlled (Focusrite Control): the
  // front shows only indicator LEDs; the actual toggles + the routing matrix live in the focus surface
  // (FocusriteControl.svelte). 48V is omitted (phantom not modeled); the two phones jacks are mono
  // (stereo-TRS-as-two-lanes is a later fidelity case). Exposed ids from the Rust `scarlett_8i6` entry:
  // 0 Gain1 · 1 Pad1 · 2 Air1 · 3 Gain2 · 4 Pad2 · 5 Air2 · 6 Phones1 · 7 Phones2 · 8–203 routing
  // crosspoints · 204 Monitor · 205 Power. Pad/Air and the crosspoints are placed by the focus surface,
  // so the guardrail unions both surfaces' coverage.
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

      <!-- Monitor: the big centre knob (drives line outs 1–2 via the Rust Monitor group). -->
      <div class="section monitor">
        <Legend text="Monitor" />
        <div class="big"><Control id={204} cap="dark" /></div>
      </div>

      <!-- Two headphone outputs, each with its own level knob + front jack. -->
      <div class="section">
        <Legend text="◎ Phones 1" />
        <Control id={6} cap="dark" />
        <Socket dir="output" id={6} />
      </div>
      <div class="section">
        <Legend text="◎ Phones 2" />
        <Control id={7} cap="dark" />
        <Socket dir="output" id={7} />
      </div>

      <span class="wordmark">Scarlett <b>8i6</b></span>
    </div>
  {/snippet}

  {#snippet back()}
    <div class="back">
      <!-- Rear line inputs 3–6 (line-level → the extra ADs → matrix). -->
      <div class="section">
        <Legend text="Line In 3–6" />
        <div class="row">
          <Socket dir="input" id={2} />
          <Socket dir="input" id={3} />
          <Socket dir="input" id={4} />
          <Socket dir="input" id={5} />
        </div>
      </div>
      <!-- Line outputs 1–4 (1–2 are the monitor pair, 3–4 direct from the matrix). -->
      <div class="section">
        <Legend text="Line Out 1–4" />
        <div class="row">
          <Socket dir="output" id={2} />
          <Socket dir="output" id={3} />
          <Socket dir="output" id={4} />
          <Socket dir="output" id={5} />
        </div>
      </div>
      <!-- S/PDIF (RCA coax) in/out — a 2-lane digital pair each. -->
      <div class="section">
        <Legend text="S/PDIF" />
        <div class="row">
          <Socket dir="input" id={6} />
          <Socket dir="output" id={1} />
        </div>
      </div>
      <!-- USB (one connector): the 8-lane send + 6-lane return cluster. -->
      <div class="section">
        <Legend text="USB" />
        <div class="row">
          <Socket dir="output" id={0} />
          <Socket dir="input" id={7} />
        </div>
      </div>
      <!-- MIDI (DIN) thru. -->
      <div class="section">
        <Legend text="MIDI" />
        <div class="row">
          <Socket dir="input" id={8} />
          <Socket dir="output" id={8} />
        </div>
      </div>
      <!-- 12V DC inlet — decorative silkscreen only (external PSU, not modeled). -->
      <div class="section">
        <Legend text="12V DC" />
        <div class="dc-inlet" aria-hidden="true"></div>
      </div>
      <!-- One switch for the whole unit — a real 8i6 is a single powered device (Rust param group). -->
      <div class="section">
        <Legend text="Power" />
        <Control id={205} />
      </div>
    </div>
  {/snippet}
</Chassis>

<style>
  /* Front: a horizontal strip of sections (two channels, monitor, two headphones), the Focusrite idiom;
     the back carries the many rear connectors. Both wrap so the (now dense) back panel stays readable at
     small in-world sizes and expands in the focus overlay. Sizes are relative + container-query based so
     the strip scales with the chassis. */
  .front,
  .back {
    display: flex;
    flex-direction: row;
    flex-wrap: wrap;
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
  /* The decorative DC barrel-jack silkscreen (no patchable port). */
  .dc-inlet {
    width: clamp(6px, 5cqw, 0.9rem);
    height: clamp(6px, 5cqw, 0.9rem);
    border-radius: 50%;
    border: 2px solid var(--ae-faceplate-ink-muted, var(--ae-text-muted));
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
