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
  <span class="label">{port.label}</span>
</div>

<style>
  /* Sizes scale with the chassis (`cqh` against the `.content` size container) but cap at the original
     rem, so a thin 1U rack unit shrinks its jacks to fit while larger devices look unchanged. */
  .jack {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: clamp(1px, 3cqh, 0.2rem);
    min-width: 0;
  }
  .connector {
    width: clamp(6px, 34cqh, 1.5rem);
    height: clamp(6px, 34cqh, 1.5rem);
    border-radius: 50%;
    background: radial-gradient(circle at 50% 35%, #444, #1a1a1a);
    border: 2px solid #0a0a0a;
    box-shadow: inset 0 0 0 3px var(--ring, #888);
  }
  /* Digital carriers read as squared connectors (e.g. coax/optical/AES housings). */
  .jack[data-domain="digital"] .connector {
    border-radius: 4px;
  }
  /* Connector colour by kind — the at-a-glance "what plugs in here". */
  .jack[data-kind="mic"] .connector {
    --ring: #4a90d9;
  }
  .jack[data-kind="line"] .connector {
    --ring: #9aa0a6;
  }
  .jack[data-kind="instrument"] .connector {
    --ring: #e08a3c;
  }
  .jack[data-kind="speaker"] .connector {
    --ring: #d6453c;
  }
  .jack[data-kind="digital"] .connector {
    --ring: #3aa0a0;
  }
  .jack[data-kind="midi"] .connector {
    --ring: #9b6cd6;
  }
  .label {
    font-size: clamp(4px, 15cqh, 0.65rem);
    color: #555;
    text-align: center;
    line-height: 1.1;
    white-space: nowrap;
  }
</style>
