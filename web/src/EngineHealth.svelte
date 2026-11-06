<script lang="ts">
  // The engine's lifecycle + real-time health, for people *and* for checks. The visible text is prose;
  // the stable hooks are the `data-testid`s and their `data-*` values, which the live-verification skill
  // (`.claude/skills/verify-live`) and `test-browser/live-hooks.spec.ts` read — rename one and that spec
  // fails. Shown in the studio's Debug menu and the workbench's Debug drawer.
  import type { SceneSession } from "./session.svelte";

  interface Props {
    session: SceneSession;
  }
  let { session }: Props = $props();

  const h = $derived(session.lastHealth);
</script>

<div class="engine-health">
  <span
    class="state state-{session.engineState}"
    data-testid="engine-status"
    data-state={session.engineState}
    data-sample-rate={session.sampleRate ?? ""}
    aria-label="engine {session.engineState}"
    title={session.fault ?? session.status}
  >
    {session.engineState}{session.sampleRate ? ` @ ${session.sampleRate} Hz` : ""}
  </span>
  <span
    data-testid="engine-overruns"
    data-value={h?.overruns ?? ""}
    data-quanta={h?.quanta ?? ""}
    aria-label="engine overruns {h?.overruns ?? 'unknown'}"
  >
    {h
      ? `${h.overruns} overrun${h.overruns === 1 ? "" : "s"} / ${h.quanta} quanta`
      : "no health yet"}
  </span>
  <span
    data-testid="engine-drops"
    data-event={h?.eventDrops ?? ""}
    data-param={h?.paramDrops ?? ""}
    aria-label="engine drops {h ? `${h.eventDrops} event, ${h.paramDrops} param` : 'unknown'}"
  >
    {h ? `drops ${h.eventDrops} event · ${h.paramDrops} param` : ""}
  </span>
</div>

<style>
  .engine-health {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0.25rem 0.9rem;
    font-variant-numeric: tabular-nums;
  }
  .state {
    color: var(--ae-text-strong);
  }
  .state-error {
    color: var(--ae-warn, #c98a2b);
  }
</style>
