# Story 5.7 part 2 — execution plan (tasks 5.7.5–5.7.10)

Part 1 of Story 5.7 (tasks 5.7.1–5.7.4) landed the faceplate system and the reduced proving
Scarlett 8i6. Building it surfaced the architectural gaps this plan fixes. The tasks are already
tracked in `IMPLEMENTATION_PLAN.md` (Story 5.7 part 2); this doc is the detailed execution plan:
the settled decisions, the code-verified starting points, and per-task designs. Decisions here were
settled with Oskari on 2026-07-03 against the real 8i6's front/rear panels.

The six tasks decompose into **three architectural gaps, two consumers, one chore**:

| Gap / consumer | Task |
| --- | --- |
| A. No device-level param concept (one exposed param → exactly one node param) | 5.7.5 |
| B. Physics frozen at compile (divider gains baked; no analog shelf; phantom has no upstream path) | 5.7.6 |
| C. Mono lanes + fixed topology above the engine (no multichannel connectors, no runtime routing) | 5.7.7 + 5.7.9 |
| Consumer: the faithful 8i6 (full I/O, S/PDIF, computer peer) | 5.7.8 |
| Consumer/chore: device dimensions pass | 5.7.10 |

## Settled decisions

1. **Execution order: 5.7.5 → 5.7.6 → 5.7.7 → 5.7.9 → 5.7.8 → 5.7.10** — matrix *before* full
   I/O, so the expanded 8i6's internal wiring is authored once around the matrix instead of as
   fixed `InternalEdge`s that 5.7.9 would immediately replace.
2. **Device-level power = catalog param groups; AD/DA also gain `powered`.** The grouping concept
   lives in the `devices` crate (the engine stays strictly per-node, per `node.rs`'s own layering
   doc). Converters need their own gate because in 5.7.8 the line-ins 3–6 reach the AD without
   passing any `GainStage` — an "off" device must not keep feeding USB.
3. **Preamp physics = a new `MicPreamp` engine node** (not params grown onto `GainStage`, which is
   used generically by monitor/phones amps and `channel_strip`). Part 1's "no new engine node" was
   part-1-only.
4. **INST/hi-Z = structural config + recompile-on-toggle** (option a). The real switch is a relay
   re-wiring the input stage — a structural change is honest. Reuses the routine `loadPatch`
   hot-swap; no hot-path cost; the runtime-re-solvable-divider alternative (dynamic `InputZ` + a
   per-block fan-out re-solve) is rejected as a hot-path mechanism nothing else needs yet.
5. **INST/AIR/PAD toggles are faithful: focus-surface only.** On the real 2nd-gen 8i6 they are
   software-controlled (Focusrite Control); the front panel has only indicator LEDs. So: passive
   LEDs on the faceplate, toggles in the device's focus surface. Consequence: a minimal
   **"Focusrite Control" focus surface is seeded in 5.7.6** (preamp switches page) and **grown in
   5.7.9** (routing matrix grid).
6. **48V phantom stays deferred** (needs the upstream phantom-supply side-graph — epic-level).
   When it lands it is a single *global* button (per the real front panel), i.e. another param
   group. Until then the faceplate **omits** the 48V button entirely (no cosmetic controls).
7. **A minimal `computer` device ships in 5.7.8** as the USB peer — without it a multichannel USB
   port is legal-connection-less and the 8i6 can't be played end-to-end.
8. **Guardrail widens to faceplate ∪ focus surfaces, via declared coverage.** Surfaces that render
   params from data (matrix grid, future menu-style digital-device UIs) can't be regex-scanned for
   literal `id={N}`; registered surfaces may declare the ids they cover and the test unions that
   with the literal scan. Not everything is a knob or slider.
9. **No schema backwards compatibility.** `SCHEMA_VERSION` bumps freely; saves are disposable
   (single user, starts from scratch).

## Code-verified starting points (recon 2026-07-03)

- **The engine already models multichannel digital.** `AudioFormat` carries `channels`
  (`engine/src/port.rs:21`), `lane_count()` returns it (`port.rs:186`), the digital edge branch
  builds one route per channel (`schedule.rs:755-758`), and an 8-channel port is tested at the
  port level (`port.rs:303-307`). The mono assumption lives **only** in the catalog node builders
  (every AD/DA constructed 1-channel) and the descriptor (`Connector::Digital` flat, no channel
  count). Gaps: no node ever declares >1 digital channel (path untested end-to-end), `Lifted`
  is analog-only, and `CompileError::ConductorMismatch`'s message says "balanced vs. unbalanced"
  (misleading for a digital channel mismatch).
- **Multi-output nodes are machinery-supported but have zero users.** Pool allocation, per-port
  bases, and step emission all iterate ports (`schedule.rs:600-634`, `832-838`); no existing node
  declares more than one output. The 5.7.9 matrix is the first user → tests land there.
- **The loading divider is a compile-time constant.** `fan_out_gains` solves each output port's
  fan-out group once at compile step 7 into constant `EdgeTransform.gain`s
  (`schedule.rs:676-770`, `electrical/divider.rs:60-91`). Fan-out couples all branches of one
  output port — you cannot re-solve one edge alone. This is why INST is structural (decision 4).
- **`BuiltDevice.params` is strictly `Vec<(NodeId, ParamId)>`** — one exposed param, one target
  (`devices/src/catalog.rs:852-865`; resolution `build.rs:381-390`). No fan-out precedent; 5.7.5
  changes this shape.
- **The routing seam is pre-designed.** `catalog.rs:18-24` (module doc): runtime-switchable
  routing "is **not** a topology change — it lives inside a node behind a control param"; and
  build-time-parameterized topology needs "an optional structural-config field on the scene
  `DeviceInstance`". 5.7.9 and 5.7.6 respectively build exactly these.
- **Analog filters: only one-pole** (`cable.rs` `OnePole`, `DcBlocker` as the analog-node
  template with `prepare(AnalogRate)` + `per_conductor`). `Biquad::high_shelf` exists but is
  digital-rate (`dsp/biquad.rs:89`); AIR reuses it fed the analog rate.
- **`GainStage::powered`** is a smoothed 0/1 multiply after gain+clip (`gain.rs:143-152`,
  `smooth_ms 5.0`) — the template for AD/DA `powered` and the MicPreamp PAD.
- **Web:** faceplate registry `device-ui.ts` (`FACEPLATES`/`FOCUS_SURFACES` + `focus.ts`
  `DEDICATED_FOCUS_SURFACES`, must stay in sync); the 8i6 is currently **not focusable** (no
  events input, no dedicated surface). Client legality (`connections.ts:80-138`) mirrors
  `build_patch` (domain → connector → cycle → dup → fan-in-replace); jack anchors key on
  `device:direction:portId` in three places (`Jack.svelte:17`, `patching.ts:42`, `:47-58`).
  Guardrail `web/test/faceplate.test.ts` regex-scans faceplate source for literal `id={N}` and
  compares hard-coded expected arrays (Scarlett-only; no auto-discovery over `FACEPLATES`).
- **Param/port ids are positional** (`catalog.rs:1026,1042,1055,1072`) — nearly every task below
  shifts ids, which ripples into faceplate `id={N}` references, guardrail arrays, and saved
  scenes (disposable, decision 9).

## The real 8i6 (from the manual panels — the 5.7.8 target)

- **Front:** 2 combo inputs (XLR+TRS) with gain knobs; INST/AIR/PAD indicator LEDs per channel
  (software-toggled); one global 48V button; MIDI + USB indicator LEDs; large MONITOR knob
  (drives line outs 1–2); **two** headphone outputs, each with its own level knob.
- **Rear:** power switch; 12V DC in (external PSU — *not* bus-powered); S/PDIF in/out (RCA coax);
  USB (one connector); MIDI in/out (DIN); LINE OUTPUTS 1–4; LINE INPUTS 3–6.
- **Channel count:** 8 in = 2 combo + 4 line + S/PDIF pair; 6 out = 4 line + S/PDIF pair
  (phones mirror output pairs). USB carries 8 up / 6 down.

---

## Task 5.7.5 — Device-level power via catalog param groups

**`devices` crate.** `CatalogEntry` gains a device-level param concept: a group param binds one
exposed control to N `(node index, ParamId)` targets.

- Node params captured by a group are **hidden from the positional walk** in `expand`
  (`catalog.rs:911-969`) — the same convention as ports consumed by `InternalEdge`s; group params
  are appended to the exposed face in declaration order. The entry's `params: &[ParamUi]` stays
  positionally aligned to the *new* face (the alignment test enforces it).
- `BuiltDevice.params` becomes one-to-many (`Vec<Vec<(NodeId, ParamId)>>` or equivalent);
  `instantiate` (`catalog.rs:976-1003`) and `build.rs:381-390` resolve every target;
  `BuiltScene::param` → the wasm `set_param` fans one value out to all handles. Descriptor
  min/max/default derive from the targets, with a catalog test asserting all targets of a group
  carry **identical decls** (else it's an authoring error).
- **Engine:** add `powered` (smoothed 0/1 multiply, `GainStage` pattern) to `AdConverter` and
  `DaConverter`. `EventThru` stays param-less — a powered-off unit still passing MIDI is a known,
  noted simplification (revisit if it grates).
- **8i6 entry:** one `Power` group targeting all `powered` params (4 GainStages + 2 AD + 1 DA);
  exposed params collapse 8 → 5 (Gain 1, Gain 2, Monitor, Phones, Power). Faceplate: the four
  back-face switches become **one power switch on the back** (matching the real rear panel);
  guardrail arrays updated. Single-node devices (`gain_stage` etc.) are untouched — ungrouped
  params still expose directly.

**Validate:** one switch silences the whole 8i6, de-clicked, no recompile; hand-check that an
"off" device emits nothing on analog *and* USB outputs; `catalog_aligns_with_exposed_face`,
`descriptors_carry_engine_truth`, the 8i6 `instantiate` remap test, and the web guardrail all
updated and green; full gate.

## Task 5.7.6 — `MicPreamp` node: PAD + AIR + INST (48V deferred)

**Engine — new `MicPreamp` node** (template: `GainStage` + `DcBlocker`):

- Params: `gain`, `powered`, `pad` (0/1 smoothed → −10 dB pre-gain multiply — smoothed value
  interpolates the attenuation, so toggling is click-free), `air` (0/1 smoothed → analog
  high-shelf, implemented as `Biquad::high_shelf` fed the analog rate, `per_conductor` +
  `replicate` like `DcBlocker`; the smoothed param crossfades dry/shelved rather than switching
  coefficients). Shelf corner/gain from Focusrite's published Air curve (≈ +4 dB @ 10 kHz —
  confirm at task time; flag as informed approximation).
- INST: a **constructor argument** selecting the input face's `InputZ` (line ≈ tens-of-kΩ vs
  instrument ≈ 1.5 MΩ — exact values from the 8i6 spec sheet at task time). Not a param.
- Hot-path discipline as usual: no alloc/panic, `f64` filter state, deterministic.
- Oracles (§9, hand-calc in comments): PAD attenuation; AIR shelf magnitude at LF vs HF; INST
  divider loss against a constructed high-output-impedance source (line-Z vs inst-Z).

**`devices` — the structural-config seam** (the `catalog.rs:18-24` design note, built):

- Scene `DeviceInstance` gains an optional `config` (string key → scalar; serde). Catalog node
  builders take the device config (signature change `fn() -> Box<dyn Node>` →
  `fn(&DeviceConfig) -> Box<dyn Node>`; existing entries ignore it). The descriptor gains a
  `configs` list (key, label, kind) so the web can render structural toggles generically.
- The 8i6's preamps become `MicPreamp`s; `inst1`/`inst2` config keys select the constructed
  `InputZ`.

**Web:**

- A config toggle is **not** a param: flipping it edits `scene.devices[i].config` and triggers the
  existing `loadPatch` hot-swap — exactly the repatch path. `SCHEMA_VERSION` bump.
- Faceplate: INST/AIR/PAD **indicator LEDs** per channel (passive, lit from config/param state).
- **Seed the "Focusrite Control" focus surface**: register `scarlett_8i6` in `FOCUS_SURFACES` +
  `DEDICATED_FOCUS_SURFACES`; v1 renders the per-channel preamp switches (INST config toggle,
  AIR/PAD params). 5.7.9 grows this same surface into the matrix.
- Guardrail: this task introduces the **declared-coverage mechanism** (decision 8) — the focus
  surface declares the param/config ids it covers; the test unions faceplate literals ∪ surface
  declarations and still demands full coverage + valid ids.

**Validate:** engine oracles green; toggling INST recompiles under sound without a glitch (the
hot-swap already de-clicks); AIR/PAD audible/hand-verified; the focus surface drives all three;
LEDs track state; full Rust gate + web suite; in-browser.

## Task 5.7.7 — Multichannel digital: connectors, lanes, mux/demux

**Engine:**

- New `DigitalMux` (N mono digital ins → one N-lane out) and `DigitalDemux` (N-lane in → N mono
  outs); formats must agree (rate/bits), checked at construction/compile.
- First end-to-end N-lane digital coverage: schedule tests for multi-lane edges through mux →
  demux; fix the `ConductorMismatch` naming/message so a digital channel mismatch reads as one.

**`devices`:**

- `PortDescriptor` gains `channels` — **derived** from the engine port's `lane_count()` (can't
  drift), serialized camelCase.
- `Connector` splits: `Digital` → `Usb`, `Spdif` (RCA coax); plus `Combo` for the front inputs
  (XLR+TRS). `connectors_compatible` grows from equality to an explicit matrix (`Combo` ~ `Xlr`,
  `QuarterInch`, itself; everything else stays equality).
- Legality (in `build_patch` **and** the web mirror `connections.ts`): digital connections
  additionally require equal channel counts.

**Web:**

- One port = one jack = one cable regardless of lane count (the jack key format is untouched);
  a lane-count badge on multichannel jacks. The 8i6's three-jack USB row collapses later
  (5.7.8) — this task keeps existing devices mono and merely makes N-lane possible.
- `connections.ts` channel-count rule + connector matrix mirrored; Vitest coverage.

**USB duplexity — settled simplification:** USB up and down are **two ports** (one N-lane output,
one M-lane input) drawn as one visual USB cluster; connecting a computer takes two cables. A true
one-gesture duplex cable is a 5.6-class fidelity item, noted there — not built now.

**Validate:** a device declares an N-lane digital port; mux→demux round-trips N lanes bit-exact
in tests; mono paths unchanged; cross-lane-count connections rejected on both sides of the
legality mirror; full gate incl. `cargo wasm`.

## Task 5.7.9 — Routing matrix (engine node + focus grid)

**Engine — `Matrix` node,** the first multi-output node:

- N mono digital ins × M mono digital outs; N×M crosspoint **gain** params (smoothed, so routing
  changes are click-free and a crosspoint can also attenuate — Focusrite Control's mixer,
  simplified to gains). Shape: `PassiveSum`'s `Vec<InputPort>` pattern + `GainStage`'s param
  plumbing; accumulate in `f64`.
- Schedule tests for the multi-output machinery (pool allocation per port, step emission,
  fan-out from multiple outputs) — this is where the untested path earns its coverage.

**`devices`:** rewire the (still-reduced) 8i6 through the matrix — preamp ADs and USB return in;
monitor/phones/USB-send out; **defaults reproduce today's fixed routing** (monitor gets USB
return; USB sends get the preamps), so behavior is unchanged until the user routes. Crosspoint
params are exposed with generated "In i → Out j" labels.

**Web:** grow the Focusrite Control focus surface with the **matrix grid** (rows = inputs, cols =
outputs; a cell drives its crosspoint param via the `DeviceHandle`). The grid renders params from
the descriptor data — covered via the declared-coverage manifest, not literal `id={N}`.

**Validate:** routing any input to any output at runtime with no recompile; defaults sound
identical to pre-matrix; engine matrix tests (unity crosspoint, sum of two ins, smoothed change);
full gate + web suite; in-browser.

## Task 5.7.8 — Full 8i6 + S/PDIF + the `computer` peer

**8i6 catalog entry grows to the real unit** (see the panel map above):

- Line inputs 3–6 (line-level `InputZ`, → 4 more mono ADs → matrix ins); line outputs 1–4 and a
  second phones out (DAs/gain stages → matrix outs); S/PDIF in/out as 2-lane `Spdif` ports via
  mux/demux; USB becomes **one 8-lane send + one 6-lane return** (`Usb`, via mux/demux),
  replacing the three mono USB jacks. Power group (5.7.5) absorbs the new nodes. Combo inputs
  get the `Combo` connector.
- Known, noted simplifications: phones jacks stay mono (stereo-TRS-as-two-lanes is a 5.6
  fidelity case); the 48V button is **omitted** until phantom is honest.
- Faceplate: full front/back per the real panels — 2 combo + gains + INST/AIR/PAD LEDs, MONITOR
  hero knob, 2 phones jacks + knobs (front); power switch, DC inlet (decorative silkscreen),
  S/PDIF, USB, MIDI, line in 3–6, line out 1–4 (back). Guardrail arrays regenerated.

**New `computer` catalog entry (minimal, playable):**

- One USB cluster mirroring the 8i6: 6-lane output (DAW playback), 8-lane input (recording).
- Behavior v1: **loopback + meters** — sends are metered per lane (readout lane), and returns
  mirror sends 1–2 (a "DAW monitoring path"), giving the classic playable loop: mic/synth →
  preamp → AD → USB → computer → USB return → DA → monitor. Exact loopback shape may be refined
  at task time; a real DAW focus surface is explicitly future work.

**Validate:** the 8i6's exposed face matches the real unit's I/O count; `instantiate` remap +
alignment tests updated; the full chain above is audible in-browser through the computer;
guardrail green; full gate.

## Task 5.7.10 — Device dimensions pass

Check every catalog `FormFactor` against real gear (the 8i6 2nd gen is ≈ 216 × 47 × 173 mm —
confirm; several Desktop entries are guesses; rackmount U-counts sanity-checked).
`catalog_carries_sane_form_factors` stays green; spatial world reads right by eye. Small; last.

---

## Cross-cutting rules for the executor

- **One task = one commit** on `e5-s7/per-device-faceplates`, full pre-push gate green
  (`cargo fmt --check && cargo lint && cargo test && cargo wasm && cargo docs` + web
  `check`/`typecheck`/`test`/`build`) before reporting done; Oskari verifies and commits.
  Large tasks (5.7.6, 5.7.7, 5.7.9) may split into engine-first / devices / web sub-commits.
- **Positional-id churn is expected** every time a device's face changes: update the faceplate
  `id={N}` references, the guardrail expectations, and bump `SCHEMA_VERSION` in the same change.
  No save migration ever (decision 9).
- **Layer rule holds:** no layout vocabulary in Rust; the catalog gains *capabilities* (groups,
  configs, channels, matrix params), the web gains *presentation*.
- **No cosmetic controls:** anything not honestly modeled is omitted (48V button, decorative DC
  inlet is silkscreen-only, not a jack).
- **Hot-path discipline** for every new node/param path: no alloc/panic/locks in `process`,
  smoothed params for anything audible, `f64` where accumulating, seeded determinism.
- After part 2 lands, sweep the transient notes: `IMPLEMENTATION_PLAN.md` Story 5.7 statuses,
  `IMPROVEMENTS.md` pointer line, and retire this doc's "open" wording.
