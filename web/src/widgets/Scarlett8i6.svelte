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
  import Col from "./layout/Col.svelte";
  import Place from "./layout/Place.svelte";
  import Row from "./layout/Row.svelte";
  import Led from "./Led.svelte";
  import Legend from "./Legend.svelte";
  import Socket from "./Socket.svelte";

  let props: DeviceUiProps = $props();
  const skin = $derived(skinFor(props.typeId));
  // makeHandle reads the (stable) props object lazily, so capture it once; untrack documents that.
  const handle = makeHandle(untrack(() => props));

  // Physical control sizes in **mm** (the panel is laid out at 1 px/mm; the world/bench zoom scales it).
  // Set once on each face as inherited CSS vars so every Legend/Led/Jack picks a real-gear size; a few
  // controls override per-instance (the big combo inputs, the small phones jacks). See the size props.
  // Measured off a real 2nd-gen 8i6: ¼" (6.3 mm) sockets are ~8 mm across (the default `--jack`); the XLR
  // combo inputs (23 mm), gain knobs (14 mm) and the monitor knob (28 mm) override per-instance below.
  const faceVars =
    "--legend: 2.6px; --led: 3px; --led-font: 2.4px; --led-gap: 0.7px; " +
    "--jack: 8px; --jack-font: 3px; --jack-gap: 1px; --jack-lane-font: 2.4px";
</script>

<Chassis {handle} flipped={props.flipped} finish={skin.finish} accent={skin.accent}>
  {#snippet front()}
    <Row fill wrap relative justify="around" align="start" gap={4} style={faceVars}>
      <!-- Two combo channels: gain knob, INST/AIR/PAD indicator LEDs (lit from config/param state;
           toggled in Focusrite Control), then the input jack. Written out per channel with **literal**
           ids so the faceplate guardrail can confirm each control/jack is placed and reachable. -->
      <Col align="start" pt={5}>
        <Socket dir="input" id={0} size={23} />
      </Col>
      <Col gap={1.5} alignSelf="stretch">
        <Row pt={2} gap={1} align="start">
          <Legend text="1" />
          <Control id={0} cap="dark" size={14} />
        </Row>
        <Row gap={1} mt="auto" pb={9}>
          <Led on={handle.config("inst1") >= 0.5} label="Inst" />
          <Led on={handle.value(2) >= 0.5} label="Air" />
          <Led on={handle.value(1) >= 0.5} label="Pad" />
        </Row>
      </Col>
      <Col align="start" pt={5}>
        <Socket dir="input" id={1} size={23} />
      </Col>
      <Col gap={1.5} alignSelf="stretch">
        <Row pt={2} gap={1} align="start">
          <Legend text="2" />
          <Control id={3} cap="dark" size={14} />
        </Row>
        <Row gap={1} mt="auto" pb={9}>
          <Led on={handle.config("inst2") >= 0.5} label="Inst" />
          <Led on={handle.value(5) >= 0.5} label="Air" />
          <Led on={handle.value(4) >= 0.5} label="Pad" />
        </Row>
      </Col>

      <!-- Monitor: the big centre knob (drives line outs 1–2 via the Rust Monitor group). The
           .monitor frame is decoration (accent hairline); the Col does the layout. -->
      <div class="monitor">
        <Col gap={1.5} px={1.5} py={2.5}>
          <Legend text="Monitor" />
          <Control id={204} cap="dark" size={28} />
        </Col>
      </div>

      <!-- Two headphone outputs, each with its own level knob + front jack. -->
      <Row alignSelf="stretch" align="start" gap={4} pt={5}>
        <Col gap={6}>
          <Row align="start">
            <Legend text="1" />
            <Control id={6} cap="dark" size={10} />
          </Row>
          <Socket dir="output" id={6} />
        </Col>
        <Col justify="center" alignSelf="stretch">
          <Legend text="🎧" />
        </Col>
        <Col gap={6}>
          <Row align="start">
            <Legend text="2" />
            <Control id={7} cap="dark" size={10} />
          </Row>
          <Socket dir="output" id={7} />
        </Col>
      </Row>

      <Place x={4} b={-6}>
        <span class="wordmark">Scarlett <b>8i6</b></span>
      </Place>
    </Row>
  {/snippet}

  {#snippet back()}
    <Row fill wrap justify="around" gap={4} style={faceVars}>
      <!-- One switch for the whole unit — a real 8i6 is a single powered device (Rust param group). -->
      <Col gap={1.5}>
        <Legend text="Power" />
        <Control id={205} size={6} />
      </Col>
      <!-- S/PDIF (RCA coax) in/out — a 2-lane digital pair each. -->
      <Col gap={1.5}>
        <Legend text="S/PDIF" />
        <Col gap={2.5}>
          <Socket dir="input" id={6} size={9} />
          <Socket dir="output" id={1} size={9} />
        </Col>
      </Col>
      <!-- USB (one connector): the 8-lane send + 6-lane return cluster. -->
      <Col gap={1.5}>
        <Legend text="USB" />
        <Col gap={2.5}>
          <Socket dir="output" id={0} size={9} />
          <Socket dir="input" id={7} size={9} />
        </Col>
      </Col>
      <!-- MIDI (DIN) thru. -->
      <Col gap={1.5}>
        <Row gap={2.5} align="start">
          <Col>
            <Legend text="Out" />
            <Socket dir="output" id={8} size={18} />
          </Col>
          <Legend text="MIDI" />
          <Col>
            <Legend text="In" />
            <Socket dir="input" id={8} size={18} />
          </Col>
        </Row>
      </Col>
      <!-- Line outputs 1–4 (1–2 are the monitor pair, 3–4 direct from the matrix). -->
      <Col gap={5} alignSelf="stretch">
        <Row gap={8}>
          <Row align="start">
            <Legend text="3" />
            <Socket dir="output" id={4} />
          </Row>
          <Row align="start">
            <Legend text="1" />
            <Socket dir="output" id={2} />
          </Row>
        </Row>
        <Legend text="Line Outputs" />
        <Row gap={8}>
          <Row align="start">
            <Legend text="4" />
            <Socket dir="output" id={5} />
          </Row>
          <Row align="start">
            <Legend text="2" />
            <Socket dir="output" id={3} />
          </Row>
        </Row>
      </Col>
      <!-- Rear line inputs 3–6 (line-level → the extra ADs → matrix). -->
      <Col gap={5} alignSelf="stretch">
        <Row gap={8}>
          <Row align="start">
            <Legend text="5" />
            <Socket dir="input" id={4} />
          </Row>
          <Row align="start">
            <Legend text="3" />
            <Socket dir="input" id={2} />
          </Row>
        </Row>
        <Legend text="Line Inputs" />
        <Row gap={8}>
          <Row align="start">
            <Legend text="6" />
            <Socket dir="input" id={5} />
          </Row>
          <Row align="start">
            <Legend text="4" />
            <Socket dir="input" id={3} />
          </Row>
        </Row>
      </Col>
    </Row>
  {/snippet}
</Chassis>

<style>
  /* Layout is composition now — <Row>/<Col>/<Place> (widgets/layout) carry the flex scaffolding, in mm.
     This block keeps only genuine *decoration* (frames, silkscreen, wordmark type), not layout. */
  .monitor {
    padding: 1.5px 2.5px;
  }
  /* The decorative DC barrel-jack silkscreen (no patchable port). */
  .dc-inlet {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    border: 1px solid var(--ae-faceplate-ink-muted, var(--ae-text-muted));
  }
  /* Brand wordmark type; "8i6" in the accent red. Position is owned by the enclosing <Place x b>. */
  .wordmark {
    font-family: var(--ae-font-display);
    font-weight: 700;
    font-size: 4px;
    letter-spacing: var(--ae-legend-spacing);
    color: var(--ae-faceplate-ink, var(--ae-text-primary));
    pointer-events: none;
  }
  .wordmark b {
    color: var(--ae-accent, var(--ae-text-primary));
  }
</style>
