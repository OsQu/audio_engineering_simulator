<script lang="ts">
  // A small engraved section legend / silkscreen caption — the shared faceplate primitive for the little
  // uppercase labels on real gear ("MONITOR", "LINE OUTPUTS", "USB"). Reads the faceplate ink tokens so it
  // contrasts with any finish; purely decorative (non-interactive). Scales with the chassis via cqh.
  interface Props {
    text: string;
    /** Physical cap-height in **mm** (real-gear silkscreen sizing, scaled by the world/bench zoom). When
     *  omitted, keeps the legacy container-relative sizing. */
    size?: number;
  }
  let { text, size }: Props = $props();
</script>

<span class="legend" style={size === undefined ? undefined : `--legend: ${size}px`}>{text}</span>

<style>
  .legend {
    font-family: var(--ae-font-display);
    font-size: var(--legend, clamp(4px, 15cqh, 0.55rem));
    font-weight: 700;
    letter-spacing: var(--ae-legend-spacing);
    text-transform: uppercase;
    color: var(--ae-faceplate-ink-muted, var(--ae-text-muted));
    white-space: nowrap;
    pointer-events: none;
  }
</style>
