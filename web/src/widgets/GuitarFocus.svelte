<script lang="ts">
  // A reusable instrument focus surface: the same structural preset selector as the physical face plus
  // the real two-octave keybed. App/Workbench already route QWERTY and Web MIDI to the topmost focused
  // device; the disabled state preserves the established rule that a cable-driven MIDI input is inert.
  import type { DeviceUiProps } from "../device-ui";
  import ConfigSelect from "./ConfigSelect.svelte";
  import Keybed from "./Keybed.svelte";

  let props: DeviceUiProps = $props();
</script>

<section class="guitar-focus">
  <header>
    <span>{props.name}</span>
    <ConfigSelect key="preset" />
  </header>
  <Keybed
    held={props.heldNotes ?? []}
    onNote={(on, note) => props.onNote?.(on, note)}
    disabled={props.notesDriven ?? false}
  />
</section>

<style>
  .guitar-focus {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    min-width: 30rem;
    max-width: 100%;
    padding: 1rem;
    box-sizing: border-box;
  }
  header {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 1rem;
    font-family: var(--ae-font-display);
    font-weight: 700;
    color: var(--ae-accent, var(--ae-text-strong));
  }
</style>
