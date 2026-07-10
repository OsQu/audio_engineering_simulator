<script lang="ts">
  // An on-screen piano keybed: two octaves of clickable keys that emit note-on/off. Purely a UI
  // affordance for the focused instrument's *events input* (Story 4.8) — it is not an engine node; the
  // note it sends is host-injected into the device's open event input, exactly like a QWERTY keypress.
  // Keys held from any source (mouse, QWERTY, MIDI) are highlighted via `held` so the two input paths
  // read as one instrument.
  interface Props {
    /** MIDI notes currently sounding (from mouse, QWERTY, or MIDI) — highlighted on the keybed. */
    held: number[];
    /** Emit a note-on (`on=true`) / note-off for a MIDI note when a key is pressed / released. */
    onNote: (on: boolean, note: number) => void;
    /** When the device's events input is cable-driven, the keybed can't perform (host notes are a
     *  no-op) — it renders inert and greyed, with a hint that the patched source plays it instead. */
    disabled?: boolean;
  }
  let { held, onNote, disabled = false }: Props = $props();

  const START = 60; // C4 — the base octave's C
  const OCTAVES = 2;
  const WHITE_SEMIS = [0, 2, 4, 5, 7, 9, 11]; // C D E F G A B
  // Black keys: their semitone offset + the index (within an octave's 7 whites) of the white they sit
  // after, so we can float them over the white-key boundaries.
  const BLACKS = [
    { semi: 1, after: 0 }, // C#
    { semi: 3, after: 1 }, // D#
    { semi: 6, after: 3 }, // F#
    { semi: 8, after: 4 }, // G#
    { semi: 10, after: 5 }, // A#
  ];

  // White keys, left to right, plus the closing C one octave up so the range ends on a C.
  const whites: number[] = [];
  for (let oct = 0; oct < OCTAVES; oct++) {
    for (const s of WHITE_SEMIS) whites.push(START + 12 * oct + s);
  }
  whites.push(START + 12 * OCTAVES);

  // Black keys with the fractional position (0..1 across the white row) of the boundary they float on.
  const blacks: Array<{ note: number; pos: number }> = [];
  for (let oct = 0; oct < OCTAVES; oct++) {
    for (const b of BLACKS) {
      blacks.push({
        note: START + 12 * oct + b.semi,
        pos: (oct * 7 + b.after + 1) / whites.length,
      });
    }
  }

  const NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
  const nameOf = (note: number): string => `${NAMES[note % 12]}${Math.floor(note / 12) - 1}`;

  // Track which keys this keybed is holding via pointer, so leaving/releasing only fires once.
  let pressed = $state<number[]>([]);
  function press(note: number): void {
    if (disabled || pressed.includes(note)) return;
    pressed = [...pressed, note];
    onNote(true, note);
  }
  function release(note: number): void {
    if (!pressed.includes(note)) return;
    pressed = pressed.filter((n) => n !== note);
    onNote(false, note);
  }
  // Release anything still held with the mouse if the keybed unmounts (e.g. the overlay closes on Esc
  // mid-press) so a note can't hang.
  $effect(() => () => {
    for (const note of pressed) onNote(false, note);
  });
</script>

<div class="keybed" class:disabled>
  <div class="keys">
    {#each whites as note (note)}
      <button
        type="button"
        class="white"
        class:held={held.includes(note)}
        aria-label={nameOf(note)}
        onpointerdown={(e) => {
          e.preventDefault();
          press(note);
        }}
        onpointerup={() => release(note)}
        onpointerleave={() => release(note)}
      ></button>
    {/each}
    {#each blacks as b (b.note)}
      <button
        type="button"
        class="black"
        class:held={held.includes(b.note)}
        style:left={`${b.pos * 100}%`}
        aria-label={nameOf(b.note)}
        onpointerdown={(e) => {
          e.preventDefault();
          press(b.note);
        }}
        onpointerup={() => release(b.note)}
        onpointerleave={() => release(b.note)}
      ></button>
    {/each}
  </div>
  {#if disabled}
    <p class="hint">Driven by <strong>MIDI In</strong> — play it from the patched controller</p>
  {:else}
    <p class="hint">
      Click keys, or type <kbd>A</kbd>–<kbd>K</kbd> to play · <kbd>Z</kbd>/<kbd>X</kbd> change octave
    </p>
  {/if}
</div>

<style>
  .keybed {
    width: 100%;
    max-width: 620px;
  }
  /* Cable-driven input: the keybed can't perform, so grey it and make the keys inert. */
  .keybed.disabled .keys {
    opacity: 0.45;
    filter: grayscale(0.6);
    pointer-events: none;
  }
  /* White keys tile the row; black keys float above at fractional boundary positions. */
  .keys {
    position: relative;
    display: flex;
    height: 150px;
    touch-action: none; /* pointer drags shouldn't scroll */
    user-select: none;
  }
  .white {
    flex: 1;
    background: linear-gradient(#fdfdfb, #e9e9e2);
    border: 1px solid #b9b9b0;
    border-radius: 0 0 4px 4px;
    box-shadow: inset 0 -3px 4px rgba(0, 0, 0, 0.12);
    cursor: pointer;
  }
  .white.held {
    background: linear-gradient(#cfe8d6, #a9d6b8);
  }
  .black {
    position: absolute;
    top: 0;
    transform: translateX(-50%);
    width: 3.4%;
    height: 62%;
    background: linear-gradient(#3a3a3a, #101010);
    border: 1px solid #000;
    border-radius: 0 0 3px 3px;
    box-shadow: 0 2px 3px rgba(0, 0, 0, 0.5);
    cursor: pointer;
  }
  .black.held {
    background: linear-gradient(#2b6b40, #17482a);
  }
  .hint {
    margin: 0.5rem 0 0;
    font-size: 0.7rem;
    color: var(--ae-text-muted);
    text-align: center;
  }
  .hint kbd {
    padding: 0 0.3em;
    border: 1px solid var(--ae-line-chip);
    border-radius: 3px;
    background: var(--ae-bg-chip);
    font-size: 0.9em;
  }
</style>
