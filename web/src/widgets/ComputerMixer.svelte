<script lang="ts">
  // The `computer`'s focus surface — the DAW mixer (Story 5.11.6). A transport (play/record/stop + a
  // digital-domain playhead), one channel strip per track (input send, arm, monitor, fader, meter), an
  // add/remove-track control, and the track → return routing crossbar (`RoutingGrid` over the `Matrix`
  // crosspoint params). Transport + track state come from the session over the `daw` seam
  // ({@link DawUi}); the crossbar routes through the `DeviceHandle` like the 8i6's routing.
  //
  // Transport model (maps onto the engine's rolling + independent record-enable): **Play** rolls with
  // record off (playback only), **Record** rolls with record on (armed tracks capture *and* recorded
  // tracks play — overdub), **Stop** halts. So the button states derive from `rolling`/`recording` alone.
  import { untrack } from "svelte";
  import { makeHandle, setDeviceHandle } from "../device-handle";
  import type { DeviceUiProps } from "../device-ui";
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

    <section>
      <div class="section-head">
        <span class="section-title">Tracks</span>
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
      <div class="tracks">
        {#each tracks as track, t (t)}
          <div class="strip" class:armed={track.armed}>
            <span class="strip-name">{track.name ?? `Track ${t + 1}`}</span>

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

            <label class="field fader">
              <input
                type="range"
                min="0"
                max="4"
                step="0.01"
                value={track.level}
                oninput={(e) => daw?.setTrackLevel(t, Number(e.currentTarget.value))}
                aria-label={`Track ${t + 1} level`}
              />
              <span class="gain">{track.level.toFixed(2)}×</span>
            </label>

            {#if trackPeaks[t] !== undefined}
              <Reading id={trackPeaks[t]} />
            {/if}

            {#if daw.waveform(t)}
              {@const peaks = daw.waveform(t)}
              {#if peaks}<Waveform {peaks} />{/if}
            {/if}
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

  .tracks {
    display: flex;
    flex-direction: row;
    flex-wrap: wrap;
    gap: 0.6rem;
  }
  .strip {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    padding: 0.6rem;
    min-width: 8rem;
    border: 1px solid var(--ae-border, #444);
    border-radius: 0.4rem;
    background: var(--ae-surface, #1c1c1c);
  }
  .strip.armed {
    border-color: #d94a4a;
  }
  .strip-name {
    font-family: var(--ae-font-ui);
    font-weight: 600;
    font-size: var(--ae-label-size);
    color: var(--ae-text-strong, #fff);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    font-family: var(--ae-font-ui);
    font-size: var(--ae-label-size);
    color: var(--ae-text-secondary, #aaa);
  }
  .field select {
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
  .fader {
    flex-direction: row;
    align-items: center;
    gap: 0.4rem;
  }
  .fader input {
    flex: 1;
    min-width: 4rem;
  }
  .gain {
    font-family: var(--ae-font-mono, monospace);
    min-width: 3rem;
    text-align: right;
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
