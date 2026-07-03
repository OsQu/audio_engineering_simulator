<script lang="ts">
  // The synth voice's faceplate: the generic `Panel` plus its signature ADSR `Screen` embellishment,
  // drawn from the live envelope params (1=attack, 2=decay, 3=sustain, 4=release). Registered as the
  // `synth_voice` faceplate (device-ui.ts), so the ADSR screen is no longer a hardcoded `typeId` branch
  // in App — the device owns its own look, and renders identically in the rack and the focus overlay.
  import type { DeviceUiProps } from "../device-ui";
  import Panel from "./Panel.svelte";
  import Screen from "./Screen.svelte";

  let props: DeviceUiProps = $props();
</script>

<Panel {...props}>
  <Screen
    attackMs={props.valueFor(1)}
    decayMs={props.valueFor(2)}
    sustain={props.valueFor(3)}
    releaseMs={props.valueFor(4)}
  />
</Panel>
