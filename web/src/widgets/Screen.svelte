<script lang="ts">
  // A synth-specific "screen": a small canvas that draws the ADSR envelope contour from the live param
  // values, redrawing as the knobs turn. Pure presentation computed from param *values* (not an engine
  // tap) — so it needs no probe surface. Opt-in per device type: App renders it only for the synth.
  interface Props {
    attackMs: number;
    decayMs: number;
    sustain: number;
    releaseMs: number;
  }
  let { attackMs, decayMs, sustain, releaseMs }: Props = $props();

  let canvas = $state<HTMLCanvasElement>();

  const W = 132;
  const H = 52;
  const PAD = 6;

  function draw(a: number, d: number, s: number, r: number): void {
    const cv = canvas;
    if (!cv) return;
    const ctx = cv.getContext("2d");
    if (!ctx) return;

    ctx.clearRect(0, 0, W, H);
    ctx.fillStyle = "#0b160b";
    ctx.fillRect(0, 0, W, H);

    // Lay the four segments out along x by their durations, with a visible sustain "hold" segment so
    // the contour is legible even at extreme settings.
    const total = a + d + r;
    const hold = Math.max(total * 0.3, 60);
    const span = total + hold || 1;
    const usable = W - 2 * PAD;
    const top = PAD;
    const bottom = H - PAD;
    const yOf = (env: number): number => bottom - env * (bottom - top);
    const xOf = (t: number): number => PAD + (t / span) * usable;

    const sus = Math.max(0, Math.min(1, s));
    const pts: Array<[number, number]> = [
      [xOf(0), yOf(0)],
      [xOf(a), yOf(1)],
      [xOf(a + d), yOf(sus)],
      [xOf(a + d + hold), yOf(sus)],
      [xOf(a + d + hold + r), yOf(0)],
    ];

    ctx.strokeStyle = "#3fe07a";
    ctx.lineWidth = 1.5;
    ctx.lineJoin = "round";
    ctx.beginPath();
    ctx.moveTo(pts[0][0], pts[0][1]);
    for (const [x, y] of pts.slice(1)) ctx.lineTo(x, y);
    ctx.stroke();
  }

  // Redraw whenever the canvas mounts or any ADSR value changes (the reads register as dependencies).
  $effect(() => {
    draw(attackMs, decayMs, sustain, releaseMs);
  });
</script>

<canvas bind:this={canvas} width={W} height={H} class="screen"></canvas>

<style>
  .screen {
    width: 132px;
    height: 52px;
    border-radius: 4px;
    border: 1px solid #061006;
    box-shadow: inset 0 0 6px rgba(0, 0, 0, 0.6);
  }
</style>
