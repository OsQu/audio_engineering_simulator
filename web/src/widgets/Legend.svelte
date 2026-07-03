<script lang="ts">
  // A small engraved section legend / silkscreen caption — the shared faceplate primitive for the little
  // uppercase labels on real gear ("MONITOR", "LINE OUTPUTS", "USB"). Reads the faceplate ink tokens so it
  // contrasts with any finish; purely decorative (non-interactive). Scales with the chassis via cqh.
  interface Props {
    text: string;
  }
  let { text }: Props = $props();
</script>

<span class="legend">{text}</span>

<style>
  .legend {
    font-family: var(--ae-font-display);
    font-size: clamp(4px, 15cqh, 0.55rem);
    font-weight: 700;
    letter-spacing: var(--ae-legend-spacing);
    text-transform: uppercase;
    color: var(--ae-faceplate-ink-muted, var(--ae-text-muted));
    white-space: nowrap;
    pointer-events: none;
  }
</style>
