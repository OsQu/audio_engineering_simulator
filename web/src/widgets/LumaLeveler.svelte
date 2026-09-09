<script lang="ts">
  // The Luma is a physical 3U optical-leveler surface. Its four hardware controls bind straight to the
  // catalog; the GR / +4 / +10 switch is intentionally local display state, selecting one of the two
  // live engine readouts without pretending to be an audio parameter.
  import { untrack } from "svelte";
  import { makeHandle } from "../device-handle";
  import { skinFor } from "../skin";
  import type { DeviceUiProps } from "../device-ui";
  import Chassis from "./Chassis.svelte";
  import Control from "./Control.svelte";
  import Legend from "./Legend.svelte";
  import Socket from "./Socket.svelte";
  import VuFace from "./VuFace.svelte";
  import Place from "./layout/Place.svelte";

  let props: DeviceUiProps = $props();
  const handle = makeHandle(untrack(() => props));
  const skin = skinFor("luma_leveler");

  type MeterMode = "gr" | "plus4" | "plus10";
  let meterMode = $state<MeterMode>("gr");

  const gainReduction = $derived(Math.max(0, handle.reading(0)));
  const outputDbu = $derived(handle.reading(1));
  const reference = $derived(meterMode === "plus10" ? 10 : 4);
  const meterFraction = $derived(
    meterMode === "gr"
      ? 1 - Math.max(0, Math.min(1, gainReduction / 40))
      : Math.max(0, Math.min(1, (outputDbu - reference + 20) / 23)),
  );
  const meterReadout = $derived(
    meterMode === "gr"
      ? `${gainReduction.toFixed(1)} dB GR`
      : `${outputDbu - reference >= 0 ? "+" : ""}${(outputDbu - reference).toFixed(1)} VU`,
  );
  const meterScale = $derived(meterMode === "gr" ? "GR" : meterMode === "plus4" ? "+4" : "+10");
</script>

<Chassis {handle} flipped={props.flipped} finish={skin.finish} accent={skin.accent}>
  {#snippet front()}
    <div class="luma-front">
      <div class="wordmark" aria-label="Luma Leveler">LUMA <span>LEVELER</span></div>
      <div class="rule top-rule"></div>

      <Place x={34} y={20} class="control-block mode-block">
        <Legend text="MODE" size={4} />
        <div class="mode-switch"><Control id={2} as="switch" size={16} /></div>
      </Place>

      <Place x={120} y={18} class="control-block gain-block">
        <Legend text="GAIN" size={4.5} />
        <Control id={1} cap="dark" size={32} />
        <span class="knob-scale">0&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;+40</span>
      </Place>

      <Place x={195} y={12} class="meter-block">
        <Legend text="LEVEL" size={4.5} />
        <VuFace fraction={meterFraction} scaleLabel={meterScale} widthMm={104} />
        <span class="meter-readout">{meterReadout}</span>
      </Place>

      <Place x={332} y={18} class="control-block peak-block">
        <Legend text="PEAK REDUCTION" size={4.5} />
        <Control id={0} cap="dark" size={32} />
        <span class="knob-scale">MIN&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;MAX</span>
      </Place>

      <Place x={424} y={19} class="meter-select">
        <Legend text="METER" size={4} />
        <div class="selector" role="radiogroup" aria-label="Meter reference">
          {#each [["gr", "GR"], ["plus4", "+4"], ["plus10", "+10"]] as [mode, label]}
            <button
              type="button"
              role="radio"
              aria-checked={meterMode === mode}
              class:active={meterMode === mode}
              onclick={() => (meterMode = mode as MeterMode)}>{label}</button
            >
          {/each}
        </div>
      </Place>

      <Place x={442} y={78} class="power-block">
        <Control id={3} as="switch" size={15} />
        <Legend text="POWER" size={4} />
      </Place>

      <div class="rule bottom-rule"></div>
      <span class="model">MODEL LL-3 · BALANCED LINE LEVELER</span>
    </div>
  {/snippet}

  {#snippet back()}
    <div class="luma-back">
      <div class="rear-wordmark">LUMA LEVELER · MODEL LL-3</div>
      <Place x={43} y={39} class="rear-port">
        <Legend text="LINE IN · 600 Ω" size={4} />
        <Socket dir="input" id={0} size={24} />
        <span>BALANCED XLR</span>
      </Place>
      <div class="tube-bank" aria-hidden="true"><i></i><i></i><i></i></div>
      <div class="transformer" aria-hidden="true"><span>OUTPUT</span><b>600 Ω</b></div>
      <Place r={43} y={39} class="rear-port output-port">
        <Legend text="LINE OUT · 600 Ω" size={4} />
        <Socket dir="output" id={0} size={24} />
        <span>BALANCED XLR</span>
      </Place>
      <span class="rear-warning">CAUTION · NO USER SERVICEABLE PARTS INSIDE</span>
    </div>
  {/snippet}
</Chassis>

<style>
  .luma-front,
  .luma-back {
    position: relative;
    width: 100%;
    height: 100%;
    box-sizing: border-box;
    overflow: hidden;
  }
  .luma-front {
    background: linear-gradient(102deg, #a7a395, #ded8c3 35%, #c3bda9 68%, #918e83);
    color: #272821;
  }
  .luma-front::before {
    content: "";
    position: absolute;
    inset: 0;
    opacity: 0.22;
    background: repeating-linear-gradient(
      92deg,
      transparent 0 3px,
      rgba(255, 255, 255, 0.55) 4px,
      transparent 5px
    );
    pointer-events: none;
  }
  .wordmark,
  .model,
  .rule,
  :global(.control-block),
  :global(.meter-block),
  :global(.meter-select),
  :global(.power-block) {
    position: absolute;
    z-index: 1;
  }
  .wordmark {
    top: 7px;
    left: 18px;
    font: 700 8px/1 var(--ae-font-display);
    letter-spacing: 0.17em;
  }
  .wordmark span {
    display: block;
    margin-top: 2px;
    font-size: 3px;
    letter-spacing: 0.38em;
  }
  .rule {
    left: 15px;
    right: 15px;
    height: 1px;
    background: rgba(38, 38, 31, 0.55);
  }
  .top-rule {
    top: 17px;
  }
  .bottom-rule {
    bottom: 13px;
  }
  :global(.control-block) {
    display: grid;
    justify-items: center;
    gap: 4px;
    text-align: center;
  }
  :global(.mode-block) {
    width: 65px;
  }
  .mode-switch {
    height: 20px;
    display: grid;
    place-items: center;
  }
  :global(.gain-block),
  :global(.peak-block) {
    width: 64px;
  }
  .knob-scale {
    font: 700 4px var(--ae-font-ui);
    letter-spacing: 0.05em;
    white-space: nowrap;
  }
  :global(.meter-block) {
    display: grid;
    justify-items: center;
    gap: 2px;
  }
  .meter-readout {
    font: 700 4px var(--ae-font-ui);
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.08em;
  }
  :global(.meter-select) {
    width: 35px;
    display: grid;
    justify-items: center;
    gap: 4px;
  }
  .selector {
    width: 100%;
    display: grid;
    gap: 2px;
  }
  .selector button {
    border: 1px solid #55564d;
    border-radius: 1px;
    padding: 1px 0;
    background: #bdb7a7;
    color: #26271f;
    font: 700 3px var(--ae-font-ui);
    cursor: pointer;
  }
  .selector button.active {
    background: #34352e;
    color: #eee7d4;
    box-shadow: inset 0 0 0 1px #10110e;
  }
  :global(.power-block) {
    display: grid;
    justify-items: center;
    gap: 3px;
  }
  .model {
    z-index: 1;
    left: 18px;
    bottom: 5px;
    font: 700 3.5px var(--ae-font-ui);
    letter-spacing: 0.12em;
  }
  .luma-back {
    background: linear-gradient(145deg, #252622, #10110f 58%, #383933);
    color: #c3c0af;
  }
  .rear-wordmark {
    position: absolute;
    top: 10px;
    left: 16px;
    font: 700 4px var(--ae-font-display);
    letter-spacing: 0.2em;
  }
  :global(.rear-port) {
    position: absolute;
    display: grid;
    justify-items: center;
    gap: 4px;
    font: 700 3.5px var(--ae-font-ui);
    letter-spacing: 0.08em;
    text-align: center;
    white-space: nowrap;
  }
  .tube-bank {
    position: absolute;
    left: 180px;
    top: 29px;
    display: flex;
    gap: 11px;
  }
  .tube-bank i {
    width: 15px;
    height: 48px;
    border: 2px solid #6f7164;
    border-radius: 8px 8px 4px 4px;
    background: linear-gradient(90deg, #11120f, #5a594a 45%, #181914);
    box-shadow: inset 0 0 0 2px #22231d;
  }
  .transformer {
    position: absolute;
    left: 276px;
    top: 37px;
    width: 47px;
    height: 38px;
    border: 2px solid #727365;
    border-radius: 50%;
    display: grid;
    place-content: center;
    text-align: center;
    font: 700 3px var(--ae-font-ui);
    letter-spacing: 0.08em;
  }
  .transformer b {
    margin-top: 3px;
    color: #e2d7bd;
    font-size: 3px;
  }
  .rear-warning {
    position: absolute;
    bottom: 7px;
    left: 0;
    width: 100%;
    text-align: center;
    font: 700 2.5px var(--ae-font-ui);
    letter-spacing: 0.18em;
    color: #77786f;
  }
</style>
