<script lang="ts">
  // A back-panel connector, rendered from a port descriptor. Styled by connector `kind` (color) and
  // carrier `domain` (round for analog/events, square-ish for digital). **Display-only** in Story 4.2 —
  // drag-to-connect patching lands in 4.4; this just establishes the jack vocabulary and labels.
  import type { PortDescriptor } from "../catalog";

  interface Props {
    /** Owning device instance id — with the port, tags the connector so the cable layer can locate it. */
    device: string;
    port: PortDescriptor;
  }
  let { device, port }: Props = $props();
</script>

<div class="jack" data-kind={port.kind} data-domain={port.domain} title={`${port.kind} · ${port.domain}`}>
  <!-- `data-jack` = "device:direction:portId" — the cable overlay measures this element's centre. -->
  <span class="connector" data-jack={`${device}:${port.direction}:${port.id}`}></span>
  {#if port.channels > 1}
    <!-- One jack carries many channels (a multichannel digital connector) — badge the lane count. -->
    <span class="lanes" title={`${port.channels} channels`}>×{port.channels}</span>
  {/if}
  <span class="label">{port.label}</span>
</div>

<style>
  /* Sizes scale with the chassis (`cqh` against the `.content` size container) but cap at the original
     rem, so a thin 1U rack unit shrinks its jacks to fit while larger devices look unchanged. */
  .jack {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: clamp(1px, 3cqh, 0.2rem);
    min-width: 0;
  }
  /* Lane-count badge for a multichannel connector, pinned to the connector's top-right corner. */
  .lanes {
    position: absolute;
    top: -0.15rem;
    right: -0.15rem;
    padding: 0 0.2rem;
    border-radius: 0.5rem;
    background: var(--ae-signal-digital);
    color: var(--ae-bg-panel, #000);
    font-family: var(--ae-font-ui);
    font-size: clamp(4px, 12cqh, 0.5rem);
    font-weight: 700;
    line-height: 1.4;
    pointer-events: none;
  }
  .connector {
    width: clamp(6px, 34cqh, 1.5rem);
    height: clamp(6px, 34cqh, 1.5rem);
    border-radius: 50%;
    /* Recessed barrel + a signal-coloured ring (the "what plugs in here" at a glance). */
    background: radial-gradient(circle at 50% 32%, var(--ae-jack-top), var(--ae-jack-bot));
    border: 2px solid var(--ae-jack-edge);
    box-shadow:
      inset 0 0 0 3px var(--ring, var(--ae-signal-line)),
      0 1px 2px rgba(0, 0, 0, 0.6);
  }
  /* Digital carriers read as squared connectors (e.g. coax/optical/AES housings). */
  .jack[data-domain="digital"] .connector {
    border-radius: var(--ae-radius-control);
  }
  /* Ring colour by connector kind, straight from the signal palette. */
  .jack[data-kind="mic"] .connector {
    --ring: var(--ae-signal-mic);
  }
  .jack[data-kind="line"] .connector {
    --ring: var(--ae-signal-line);
  }
  .jack[data-kind="instrument"] .connector {
    --ring: var(--ae-signal-instrument);
  }
  .jack[data-kind="speaker"] .connector {
    --ring: var(--ae-signal-speaker);
  }
  .jack[data-kind="digital"] .connector {
    --ring: var(--ae-signal-digital);
  }
  .jack[data-kind="midi"] .connector {
    --ring: var(--ae-signal-midi);
  }
  .label {
    font-family: var(--ae-font-ui);
    font-size: clamp(4px, 15cqh, 0.65rem);
    letter-spacing: var(--ae-label-spacing);
    color: var(--ae-text-muted);
    text-align: center;
    line-height: 1.1;
    white-space: nowrap;
  }
</style>
