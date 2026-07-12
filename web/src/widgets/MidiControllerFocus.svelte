<script lang="ts">
  // The MIDI controller's focus surface: a large **playable** keybed — the instrument you actually play.
  // Pressing a key host-injects a note into the controller's open MIDI-In event input; its `EventThru`
  // node passes it straight out the MIDI-Out cable to whatever synth is patched downstream. (The in-world
  // faceplate's little keybed is silkscreen only; this is the real one.) The note-play seam — the held
  // notes, the per-note callback, and whether the input is cable-driven — is threaded in through the
  // focus-only `DeviceUiProps` fields that `App`/`Workbench` populate for the focused device.
  //
  // This is a composable focus piece, not the old generic "any playable device earns a keybed" behaviour:
  // a device gets an on-screen keybed only by composing one here, in its own focus surface.
  import type { DeviceUiProps } from "../device-ui";
  import Keybed from "./Keybed.svelte";

  let props: DeviceUiProps = $props();
</script>

<div class="controller">
  <span class="title">Keys</span>
  <Keybed
    held={props.heldNotes ?? []}
    onNote={(on, note) => props.onNote?.(on, note)}
    disabled={props.notesDriven ?? false}
  />
</div>

<style>
  .controller {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    padding: 1rem;
    min-width: 30rem;
    max-width: 100%;
    box-sizing: border-box;
  }
  .title {
    font-family: var(--ae-font-display);
    font-weight: 700;
    letter-spacing: var(--ae-legend-spacing);
    text-transform: uppercase;
    font-size: var(--ae-legend-size, 0.8rem);
    color: var(--ae-accent, var(--ae-text-strong));
  }
</style>
