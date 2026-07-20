<script lang="ts">
  // The `computer`'s focus surface — the DAW mixer (Story 5.11.6), laid out like a real DAW: a
  // **transport** bar, an **arrangement** where every track's waveform is a bare horizontal lane on
  // one shared timeline with a single playhead sweeping across all of them, and a **mixer (MCP)** below
  // where each track is a channel strip (input send, arm, monitor, a real vertical fader with the track
  // label + value beneath it, and a post-fader meter). Routing and the send/return buses follow. All
  // transport + track state comes from the session over the `daw` seam ({@link DawUi}); the crossbar
  // routes through the `DeviceHandle` like the 8i6's routing.
  //
  // Transport model (maps onto the engine's rolling + independent record-enable): **Play** rolls with
  // record off (playback only), **Record** rolls with record on (armed tracks capture *and* recorded
  // tracks play — overdub), **Stop** halts. So the button states derive from `rolling`/`recording` alone.
  import { untrack } from "svelte";
  import type { ParamDescriptor } from "../catalog";
  import { makeHandle, setDeviceHandle } from "../device-handle";
  import type { DeviceUiProps } from "../device-ui";
  import type { TrackUi } from "../scene-store";
  import Fader from "./Fader.svelte";
  import Reading from "./Reading.svelte";
  import RoutingGrid from "./RoutingGrid.svelte";
  import Waveform from "./Waveform.svelte";

  let props: DeviceUiProps = $props();
  setDeviceHandle(makeHandle(untrack(() => props)));

  const daw = $derived(props.daw);
  const transport = $derived(daw?.transport);
  const tracks = $derived(daw?.tracks ?? []);
  const sends = $derived(daw?.sends ?? 0);
  const rolling = $derived(transport?.rolling ?? false);
  const recording = $derived(transport?.recording ?? false);

  // Readout ids per bank, matched by the descriptor's generated labels ("Send 1 Peak", "Track 1 Peak",
  // …). Track strips show their track's peak; the send/return rows show each lane's peak. Variable ids
  // (not literal) so the faceplate-coverage scan stays clean — every param is a crossbar crosspoint.
  const peaksMatching = (prefix: string): number[] =>
    (props.readouts ?? [])
      .filter((r) => r.label.startsWith(prefix) && r.label.endsWith(" Peak"))
      .map((r) => r.id);
  const trackPeaks = $derived(peaksMatching("Track "));
  const sendPeaks = $derived(peaksMatching("Send "));
  const returnPeaks = $derived(peaksMatching("Return "));

  // The in-sim digital-domain rate the playhead counts in (128 samples/block @ 48 kHz). Only used to
  // render the playhead as time — the transport itself is clocked entirely inside the simulation.
  const DIGITAL_RATE_HZ = 48000;

  // One shared timeline for the whole arrangement: long enough to hold the longest take and the current
  // playhead, with a 4 s minimum so an empty session still shows a ruler. Every lane's clip width and the
  // single playhead map [0, timelineSamples] → [0%, 100%] of the lanes column — so one play-ahead drives
  // them all (no per-lane cursor).
  const MIN_TIMELINE_SAMPLES = DIGITAL_RATE_HZ * 4;
  const timelineSamples = $derived.by(() => {
    let max = MIN_TIMELINE_SAMPLES;
    for (let t = 0; t < tracks.length; t++) {
      const wf = daw?.waveform(t);
      if (wf && wf.samples > max) max = wf.samples;
    }
    return Math.max(max, transport?.playhead ?? 0);
  });
  const playheadPct = $derived(((transport?.playhead ?? 0) / timelineSamples) * 100);
  // Whole-second ruler ticks strictly inside the timeline (the exact end tick is dropped so labels
  // never overflow the right edge).
  const rulerSecs = $derived(
    Array.from({ length: Math.ceil(timelineSamples / DIGITAL_RATE_HZ) }, (_, i) => i).filter(
      (s) => s * DIGITAL_RATE_HZ < timelineSamples,
    ),
  );
  const secPct = (sec: number): number => ((sec * DIGITAL_RATE_HZ) / timelineSamples) * 100;

  // A synthetic descriptor so a track's level rides the shared `Fader` widget (its label carries the
  // track name, so the name + value render beneath the fader — the "labels down in the mixer" bit). The
  // level is driven over the DAW seam (`setTrackLevel`), not as an exposed param, so id is unused.
  function levelParam(t: number, track: TrackUi): ParamDescriptor {
    return {
      id: 0,
      label: track.name ?? `Track ${t + 1}`,
      unit: "×",
      kind: "fader",
      min: 0,
      max: 4,
      default: 1,
    };
  }

  function formatPlayhead(samples: number): string {
    const total = samples / DIGITAL_RATE_HZ;
    const m = Math.floor(total / 60);
    const s = Math.floor(total % 60);
    const cs = Math.floor((total * 100) % 100);
    return `${m}:${s.toString().padStart(2, "0")}.${cs.toString().padStart(2, "0")}`;
  }

  function play(): void {
    daw?.setRecordEnabled(false);
    daw?.play();
  }
  function record(): void {
    daw?.setRecordEnabled(true);
    daw?.play();
  }
  function stop(): void {
    daw?.stop();
  }
  function rewind(): void {
    daw?.seek(0);
  }
</script>

{#if daw}
  <div class="mixer">
    <section class="transport">
      <div class="buttons">
        <button
          type="button"
          class="tp"
          onclick={rewind}
          aria-label="Rewind to start"
          title="To start">⏮</button
        >
        <button
          type="button"
          class="tp"
          class:on={rolling && !recording}
          onclick={play}
          aria-label="Play"
          title="Play">▶</button
        >
        <button
          type="button"
          class="tp rec"
          class:on={recording}
          onclick={record}
          aria-label="Record"
          title="Record">⏺</button
        >
        <button
          type="button"
          class="tp"
          class:on={!rolling}
          onclick={stop}
          aria-label="Stop"
          title="Stop">⏹</button
        >
      </div>
      <span class="playhead" class:live={rolling}>{formatPlayhead(transport?.playhead ?? 0)}</span>
      <span class="state">{recording ? "recording" : rolling ? "playing" : "stopped"}</span>
    </section>

    <!-- Arrangement: bare waveform lanes stacked on one shared timeline, one playhead across them all. -->
    <section>
      <div class="section-head">
        <span class="section-title">Arrangement</span>
        <div class="track-count">
          <button
            type="button"
            onclick={() => daw?.setTrackCount(tracks.length - 1)}
            disabled={tracks.length <= 1}
            aria-label="Remove track">−</button
          >
          <span>{tracks.length}</span>
          <button
            type="button"
            onclick={() => daw?.setTrackCount(tracks.length + 1)}
            aria-label="Add track">+</button
          >
        </div>
      </div>
      <div class="arrange">
        <div class="ruler">
          {#each rulerSecs as sec (sec)}
            <span class="tick" style={`left:${secPct(sec)}%`}>{sec}s</span>
          {/each}
        </div>
        <div class="lanes">
          {#each tracks as track, t (t)}
            {@const wf = daw.waveform(t)}
            <div class="lane" class:armed={track.armed}>
              <span class="lane-index">{t + 1}</span>
              {#if wf && wf.samples > 0}
                <div class="clip" style={`width:${(wf.samples / timelineSamples) * 100}%`}>
                  <Waveform peaks={wf.peaks} />
                </div>
              {/if}
            </div>
          {/each}
          <div class="playhead-line" class:live={rolling} style={`left:${playheadPct}%`}></div>
        </div>
      </div>
    </section>

    <!-- Mixer (MCP): one channel strip per track — send, arm/monitor, vertical fader (label + value), meter. -->
    <section>
      <span class="section-title">Mixer</span>
      <div class="mcp">
        {#each tracks as track, t (t)}
          <div class="mstrip" class:armed={track.armed}>
            <label class="field">
              <span>In</span>
              <select
                value={track.input}
                onchange={(e) => daw?.setTrackInput(t, Number(e.currentTarget.value))}
              >
                {#each Array.from({ length: sends }, (_, i) => i) as lane (lane)}
                  <option value={lane}>Send {lane + 1}</option>
                {/each}
              </select>
            </label>

            <div class="toggles">
              <button
                type="button"
                class="arm"
                class:active={track.armed}
                onclick={() => daw?.setTrackArmed(t, !track.armed)}
                aria-pressed={track.armed}
                title="Record-arm">●</button
              >
              <button
                type="button"
                class="mon"
                class:active={track.monitoring}
                onclick={() => daw?.setTrackMonitoring(t, !track.monitoring)}
                aria-pressed={track.monitoring}
                title="Input monitoring">M</button
              >
            </div>

            <div class="fader-meter">
              <Fader
                param={levelParam(t, track)}
                value={track.level}
                onChange={(v) => daw?.setTrackLevel(t, v)}
                size={110}
              />
              {#if trackPeaks[t] !== undefined}
                <Reading id={trackPeaks[t]} vertical />
              {/if}
            </div>
          </div>
        {/each}
      </div>
    </section>

    <section>
      <span class="section-title">Routing</span>
      <p class="hint">Track → return bus routes — how each track folds into the monitor returns.</p>
      <RoutingGrid params={props.params} />
    </section>

    {#if sendPeaks.length > 0 || returnPeaks.length > 0}
      <section class="buses">
        <div class="bus">
          <span class="section-title">Sends</span>
          <div class="meters">
            {#each sendPeaks as id (id)}<Reading {id} />{/each}
          </div>
        </div>
        <div class="bus">
          <span class="section-title">Returns</span>
          <div class="meters">
            {#each returnPeaks as id (id)}<Reading {id} />{/each}
          </div>
        </div>
      </section>
    {/if}
  </div>
{/if}

<style>
  .mixer {
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
    padding: 1rem;
    min-width: 30rem;
    max-width: 100%;
    box-sizing: border-box;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .section-title {
    font-family: var(--ae-font-display);
    font-weight: 700;
    letter-spacing: var(--ae-legend-spacing);
    text-transform: uppercase;
    font-size: var(--ae-legend-size, 0.8rem);
    color: var(--ae-accent, var(--ae-text-strong));
  }
  .hint {
    margin: 0;
    font-family: var(--ae-font-ui);
    font-size: var(--ae-label-size);
    color: var(--ae-text-secondary, var(--ae-text-primary));
  }

  .transport {
    flex-direction: row;
    align-items: center;
    gap: 1rem;
  }
  .buttons {
    display: flex;
    gap: 0.3rem;
  }
  .tp {
    font-size: 1.1rem;
    width: 2.2rem;
    height: 2.2rem;
    border-radius: 0.35rem;
    border: 1px solid var(--ae-border, #555);
    background: var(--ae-surface, #222);
    color: var(--ae-text-primary, #ddd);
    cursor: pointer;
  }
  .tp.on {
    background: var(--ae-accent, #4a90d9);
    color: #fff;
  }
  .tp.rec.on {
    background: #d94a4a;
    color: #fff;
  }
  .playhead {
    font-family: var(--ae-font-mono, monospace);
    font-size: 1.2rem;
    color: var(--ae-text-strong, #fff);
  }
  .playhead.live {
    color: var(--ae-accent, #4a90d9);
  }
  .state {
    font-family: var(--ae-font-ui);
    font-size: var(--ae-label-size);
    color: var(--ae-text-secondary, #999);
  }

  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .track-count {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-family: var(--ae-font-mono, monospace);
  }
  .track-count button {
    width: 1.6rem;
    height: 1.6rem;
    border-radius: 0.3rem;
    border: 1px solid var(--ae-border, #555);
    background: var(--ae-surface, #222);
    color: var(--ae-text-primary, #ddd);
    cursor: pointer;
  }
  .track-count button:disabled {
    opacity: 0.4;
    cursor: default;
  }

  /* Arrangement — the shared-timeline waveform stack. */
  .arrange {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--ae-border, #444);
    border-radius: 0.4rem;
    background: var(--ae-surface, #1c1c1c);
    overflow: hidden;
  }
  .ruler {
    position: relative;
    height: 1.1rem;
    border-bottom: 1px solid var(--ae-border, #444);
    background: var(--ae-surface-2, #151515);
  }
  .tick {
    position: absolute;
    top: 0;
    font-family: var(--ae-font-mono, monospace);
    font-size: 0.6rem;
    color: var(--ae-text-secondary, #888);
    padding-left: 0.15rem;
    border-left: 1px solid var(--ae-border, #444);
    height: 100%;
    line-height: 1.1rem;
  }
  .lanes {
    position: relative;
    display: flex;
    flex-direction: column;
  }
  .lane {
    position: relative;
    height: 2.6rem;
    border-bottom: 1px solid var(--ae-border, #333);
  }
  .lane:last-child {
    border-bottom: none;
  }
  .lane.armed {
    background: rgba(217, 74, 74, 0.08);
  }
  .lane-index {
    position: absolute;
    top: 0.1rem;
    left: 0.2rem;
    z-index: 1;
    font-family: var(--ae-font-mono, monospace);
    font-size: 0.6rem;
    color: var(--ae-text-secondary, #777);
    opacity: 0.7;
    pointer-events: none;
  }
  .clip {
    height: 100%;
    /* The lane's `Waveform` fills the clip's full height (overriding its thumbnail default). */
    --wave-h: 100%;
  }
  .clip :global(.wave) {
    border-radius: 0;
    background: transparent;
  }
  .playhead-line {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    background: var(--ae-text-strong, #fff);
    opacity: 0.55;
    pointer-events: none;
  }
  .playhead-line.live {
    background: var(--ae-accent, #4a90d9);
    opacity: 0.9;
  }

  /* Mixer (MCP) — channel strips side by side, scrolling if they overflow. */
  .mcp {
    display: flex;
    flex-direction: row;
    gap: 0.75rem;
    overflow-x: auto;
    padding-bottom: 0.3rem;
  }
  .mstrip {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.4rem;
    padding: 0.55rem 0.4rem;
    width: 5.25rem;
    box-sizing: border-box;
    border: 1px solid var(--ae-border, #444);
    border-radius: 0.4rem;
    background: var(--ae-surface, #1c1c1c);
    flex: 0 0 auto;
  }
  .mstrip.armed {
    border-color: #d94a4a;
  }
  .fader-meter {
    display: flex;
    flex-direction: row;
    align-items: flex-start;
    gap: 0.35rem;
    padding: 0.2rem 0;
    /* Match the vertical channel meter to the fader's throw height (size=110 ⇒ ~110px). */
    --vmeter-h: 6.9rem;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    width: 100%;
    font-family: var(--ae-font-ui);
    font-size: var(--ae-label-size);
    color: var(--ae-text-secondary, #aaa);
    align-items: center;
  }
  .field select {
    width: 100%;
    background: var(--ae-surface-2, #2a2a2a);
    color: var(--ae-text-primary, #ddd);
    border: 1px solid var(--ae-border, #555);
    border-radius: 0.25rem;
    padding: 0.15rem;
  }
  .toggles {
    display: flex;
    gap: 0.3rem;
  }
  .toggles button {
    width: 1.8rem;
    height: 1.8rem;
    border-radius: 0.3rem;
    border: 1px solid var(--ae-border, #555);
    background: var(--ae-surface, #222);
    color: var(--ae-text-secondary, #888);
    cursor: pointer;
  }
  .arm.active {
    background: #d94a4a;
    color: #fff;
    border-color: #d94a4a;
  }
  .mon.active {
    background: var(--ae-accent, #4a90d9);
    color: #fff;
    border-color: var(--ae-accent, #4a90d9);
  }

  .buses {
    flex-direction: row;
    gap: 2rem;
    flex-wrap: wrap;
  }
  .bus {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  .meters {
    display: flex;
    flex-direction: row;
    flex-wrap: wrap;
    align-items: center;
    gap: clamp(0.4rem, 2cqw, 1.2rem);
  }
</style>
