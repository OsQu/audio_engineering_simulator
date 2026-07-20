# Epic 5 — Device Fidelity & the Digital Hub — Design Notes & Delivery Record

Companion archive to `IMPLEMENTATION_PLAN.md`. Epic 5 grew device fidelity and the digital medium on the
proven engine + UI: it made a Focusrite **Scarlett 8i6** a faithful device (a per-device faceplate system,
device power, preamp physics, +48 V phantom, multichannel USB, a runtime routing matrix), and grew the
**`computer`** it plugs into from a fixed monitoring loopback into a **minimal DAW** (duplex digital links,
dynamic I/O enumeration, multitrack recording to OPFS). Stories **5.7–5.11 are all done**; the plan keeps a
tight summary of Epic 5, and **this file is the full record** — the *settled design notes* (with the
rejected alternatives and the reasoning that justified each choice), the per-task breakdown, and the
per-task *Delivered* / *Deviations* notes.

Read this when a later epic's design decision turns on **why** Epic 5 was built the way it was, or when you
need the exact API/behavior of something Epic 5 shipped. For *what exists and what binds you going forward*,
the plan's Epic 5 summary is enough; come here for the depth behind it.

> **Numbering note.** Epic 5 was originally titled "Breadth & Challenges" and sketched Stories 5.1–5.11.
> Only 5.7–5.11 were built (the device-fidelity/DAW arc that named this epic in hindsight); the unbuilt
> roadmap-scale breadth (the original 5.1–5.6 + Story 5.9's deferred L2-forwarding layer) moved to **Epic 7
> — Breadth & Challenges**. The original story sketches and framing are preserved verbatim below under
> "Epic 5 framing (as originally written)" for the historical record; the live plan for that work is Epic 7.

The plan's section ordering (Goal → Watch out → Design notes → Tasks → Validate → Delivered) is preserved
per story.

---

## Epic 5 — Breadth & Challenges

**Goal:** grow device coverage and the medium (routing, studio wiring, live sound scaling toward large
venues), deepen DSP and AD/DA, and add the game layer.

**Exit criteria:** the same engine credibly supports studio, routing, and live-sound scenarios; structured
challenges layer on top of the sandbox.

**Watch-outs:** multi-core only if profiling at scale demands it (single core covers stadium on the napkin).
Keep device transforms understandable — spend the realism budget on the volts-and-converters layer.

**Notes:** The stories in this epics are not related to each other unless \*otherwise stated. We can do them in any order or only do part of them

_Tasks to be elaborated when we reach this Epic._

- **Story 5.1** — More devices: deeper mixer, more processors, patchbay, more converters.
- **Story 5.2** — Routing & live-sound scenarios at scale (multi-core partition of the schedule if
  needed). Includes **networked audio** (Dante/AES67) as a carrier: digital-audio sample streams over
  an IP transport with its own routing, subscriptions, latency, and a network clock (PTP) — modeled
  "TCP/IP layer upwards" (network behavior + encoding), reusing the `Sample` lane and the clock-domain
  machinery, with the transport/subscription model as the net-new piece. _The link/forwarding layer
  **beneath** this (bidirectional links + simplified L2 switching) is **Story 5.9** — the two stack:
  5.9 is layer 2, 5.2 is IP-and-up._
- **Story 5.3** — Deeper DSP and deeper AD/DA modeling as needed. Includes **clock domains, sync, and
  sample-rate conversion** — the payoff of the "crossing any clock = resample" rate model (Story 1.1)
  and the carrier/clock seam (Story 1.6). This is where the **emergent clock model** (the clock-domains
  decision below) is built: per-domain oscillators as real rates against the analog continuum, elastic
  FIFOs at async boundaries that genuinely slip, word-clock master/slave, recovered-vs-dedicated
  clocking, and a **fractional sample-rate converter** as the honest fix (also what lets a 44.1 k
  device meet a 48 k one). "Fix the clocking" (set a master, slave the rest) becomes a diagnostic
  challenge alongside "fix the hum".
- **Story 5.4** — Challenge / diagnostic-scenario framework on the sandbox. Includes the
  **ground-topology-derived hum** decision below ("fix the hum" is a named challenge scenario).
- **Story 5.5** — Optional schematic / node-graph view over the same model.
- **Story 5.6** — Device fidelity & the corner cases it forces. As devices get more faithful (a real
  audio interface, a proper mixer with sends/inserts, a patchbay), they break simplifying assumptions the
  earlier UI baked in; this is where those get collected and resolved (distinct from 5.1's breadth-of-
  coverage — this is depth-of-realism). The layer rule holds throughout: the **catalog owns which ports
  exist** (specs), the **web owns how they're drawn** (look & feel). _First known case — mixed-face I/O:_
  Story 4.8's `skin.ioFace` is **per-device** (a whole device's jacks on one face — `back` default,
  `front` for the MIDI controller). Real gear splits them — an audio interface puts **hi-Z instrument
  inputs on the front** and line/digital/word-clock on the back. **Resolution: Story 5.7** — once a
  device authors its own faceplate as a Svelte component, face assignment is simply _which face's markup
  you write a jack into_, so the mixed-face problem dissolves (no `ioFace` per-port resolver needed; an
  earlier sketch of that resolver is superseded). The audio interface that forces this is 5.7's proving
  device. Further fidelity corner cases accrue here as they surface.
- **Story 5.7** — Per-device faceplate UIs: each device authors its own look & feel. ✅ **Complete**
  (parts 1 + 2; merged to `main`).
- **Story 5.8** — 48V phantom power: balanced preamp front-end, a patchable condenser mic, and the
  compile-time phantom DC operating-point solve (retiring the mic's self-`powered` flag).
  ✅ **Complete** (on `e5-s8/phantom-power`, Validate met; see the Story block below).
- **Story 5.9** — **Bidirectional digital transport & simplified L2 forwarding.** Part 1 — **duplex
  single-connector links** (USB-C = one jack, both directions) + **topology-derived delay** (`compile`
  auto-breaks digital cycles) — ✅ **delivered & verified in-browser** (see the Story 5.9 block below).
  The L2 forwarding layer proper — addressed flows, switches, clock domains — remains **deferred**
  (feeds Story 5.2/5.3; the design work is parked in the Story 5.9 block).
- **Story 5.10** — **Dynamic computer I/O**: the computer stops hardcoding the 8i6's USB shape and
  adapts to the attached interface's **published** channel counts (lane-aware engine nodes +
  config-driven host enumeration). ✅ **Done** (see the Story block below).
- **Story 5.11** — **Computer as a minimal DAW**: the computer grows arbitrary mono tracks that arm to
  USB sends, record to WAV files on disk (OPFS), and play back to USB returns through an in-sim routing
  matrix + simple level mixer, with a transport (play/stop/record) clocked by the **in-simulation digital
  domain**. Host is dumb byte storage; the sim owns all audio. ✅ **Done** (see the Story block below).

### Story 5.7 — Per-device faceplate UIs — ✅ **Complete**

_Goal:_ Give the web layer a way for **each device to author its own faceplate** — real look-and-feel per
device — while the engine/`devices` catalog stays specs-only. Today every device is drawn by **one
generic, flow-based renderer** (`web/src/widgets/Panel.svelte`: flexbox controls in exposed-param order,
flexbox In/Out jack groups) dressed by a thin `skin.ts` (three faceplate finishes + knob caps + a
**per-device** `ioFace`). That cannot express real gear: controls positioned relative to their jacks, a
big centre monitor knob, section legends ("MONITOR", "LINE OUTPUTS"), brand identity (a red Focusrite
chassis, a "Teletronix" wordmark), or **mixed-face I/O** (front instrument inputs, rear line/MIDI/USB).
Anchors to PROJECT_PLAN §4 (device/port domain model) and §7 (UI a pure consumer of the published engine
API), and to the Epic-4 settled layer rule (the **catalog owns which ports/params exist**; the **web owns
how they're drawn**). Proven end-to-end on a **simplified Focusrite Scarlett 8i6** — the first mixed-face,
branded device.

_Done (both parts; merged to `main`):_

- **Part 1 (5.7.1–5.7.4) — the faceplate system.** A device-UI registry (`typeId → component`, else the
  generic `Panel`), a `DeviceHandle` context + `Chassis` bezel publishing it, **bound widgets**
  (`Control`/`Socket`/`Reading`/`ConfigSwitch`) that bind by id, focus surfaces generalized per device, and
  a static **coverage guardrail** (`web/test/faceplate.test.ts`) proving every exposed param/port is placed
  and only valid ids referenced. Proven on the (then reduced) Focusrite Scarlett 8i6 with its red-chassis
  brand accent.
- **Part 2 (5.7.5–5.7.10) — the fidelity needed to make the 8i6 real**, each an engine/devices concept the
  UI then consumes:
  - **5.7.5 device-level power** — a catalog **param group** binds one exposed control to N node params;
    `AdConverter`/`DaConverter` gained a smoothed `powered` gate. One Power switch silences the whole unit.
  - **5.7.6 preamp physics** — a `MicPreamp` node (PAD −10 dB, AIR high-shelf, INST/hi-Z as a **structural
    config** that recompiles), plus the scene `config` seam and generic `configs` descriptor; the Focusrite
    Control focus surface drives them (48V still deferred to the phantom side-graph).
  - **5.7.7 multichannel digital** — `DigitalMux`/`DigitalDemux`, per-port lane counts on the descriptor, a
    `Combo` connector, and the first end-to-end N-lane digital coverage.
  - **5.7.9 runtime routing matrix** — a params-driven `Matrix` node (route/mix/mute per crosspoint, no
    recompile), surfaced as the data-driven `RoutingGrid` in the focus view.
  - **5.7.8 full 8i6 + `computer` peer** — the 8i6 grown to the real unit (9 in / 9 out: 2 combo + 4 line +
    S/PDIF + USB + MIDI; 14×14 matrix; 206-param face), a generated crosspoint-label mechanism (`GridSpec`),
    and a minimal `computer` USB peer (per-lane send meters + loopback). Closing the monitoring loop through
    the computer was a graph cycle, so we added a **delayed-edge** primitive (`Graph::connect_delayed`, cut
    from the topo sort, served from the persistent pool → one block of round-trip latency; the schedule
    stays a DAG). The default scene is now this playable loop.
  - **5.7.10 device dimensions** — form factors corrected against real gear (8i6 → 216×47×173 mm; a laptop
    `computer`; the rest sanity-checked).
- **Layer rule held throughout:** the Rust catalog gained no layout vocabulary; the web stayed a pure
  consumer binding by id.

_(The detailed part-2 execution plan and the round-trip-latency design write-up lived in
`one_off_plans/story_5_7_part2_plan.md` and `one_off_plans/roundtrip_latency_plan.md`; both are retired now
that the work is captured here.)_

_Watch out:_

- **Layer rule, unchanged.** The Rust catalog gains **no layout vocabulary** (positions/faces/colours) —
  the Story 4.2 decision that rejected descriptor layout fields still holds. A faceplate references
  params/ports/readouts **by id**; all appearance lives in the web component. A task that wants a Rust
  layout field is a bug, not a shortcut.
- **No cosmetic controls.** A faceplate can only draw controls that map to **real params** (the
  power-as-control ethos: "don't flag what should emerge"). The 8i6 therefore exposes only **gain + power**
  on its preamps; its INST/AIR/PAD/48V switches are **omitted, not faked**, because none can be honestly
  modeled in today's engine (see the deferred-preamp-physics design note). **No new engine node this
  Story** — the preamps reuse the existing `GainStage`.
- **Fallback parity.** After the `Chassis` refactor, the generic `Panel` (every un-authored device) must
  render and flip **identically** to today — verified in-browser. The registry defaults to `Panel`, so
  nothing existing changes silently.
- **Signal-type split intact.** The 8i6's internal AD/DA are the only analog↔digital bridges (§5); its
  "USB" is modeled as digital ports, not a magic passthrough.
- **`$state.snapshot` at the worklet boundary still holds** — the `DeviceHandle` only repackages App's
  existing `set_param` path; no new `postMessage` shape.

_Design notes (settled at planning):_

- **Component over coordinate-map (the headline decision).** A device optionally registers **its own
  Svelte component** as its faceplate, composing the shared skeuomorphic widgets
  (`Knob`/`Fader`/`Switch`/`Jack`/`Meter`) + design-system `--ae-*` tokens but arranging them with **its
  own scoped CSS** (grid/flex/absolute; absolute px is safe — the world's single zoom transform scales
  it). _Rejected: a normalized-coordinate / layout-data model_ (`paramId→{x,y}` on the skin) — a component
  gives full CSS expressiveness, free-form text/legends/logos as plain HTML, Svelte's automatic
  per-component style scoping, and it makes the **front/back-face problem dissolve** (a jack's face = which
  snippet it is written in — retiring the Story 5.6 `ioFace`-resolver sketch). The generic `Panel` stays
  the **fallback** for un-authored gear.
- **Plumbing.** A **device-UI registry** (`typeId → component`, else `Panel`; mirrors
  `skin.ts`/`focus.ts`); a **`DeviceHandle` context** packaging App's existing per-device
  `valueFor`/`readingFor`/`onParam` so a faceplate binds by id with no `postMessage` plumbing; **bound
  wrappers** `Control`/`Socket`/`Reading` (reference an id, pick the widget by descriptor `kind`, keep
  `Jack`'s `data-jack` anchor measurement working wherever placed); a **`Chassis`** primitive owning the
  shared bezel + 3-D flip (and setting the handle context) so a device authors only face _contents_. The
  generic `Panel` is **rebuilt on `Chassis`** (one flip implementation). _Rejected: leaving `Panel`
  untouched_ (two flip impls to keep in sync).
- **Preamp physics deferred — why the 8i6 shows only gain + power (settled after a code check).** The 8i6
  reuses the existing `GainStage` for its preamps; **INST/AIR/PAD/48V are omitted** because none is
  honestly modelable in today's engine, and the "no cosmetic controls" ethos forbids faking them:
  - **INST/hi-Z** would change the preamp's input impedance, but the loading divider is **baked at
    compile** into the edge transform (`schedule.rs`, from the port's static `InputZ`) — so a hi-Z switch
    is **structural** (needs a recompile-on-toggle, like repatching), not a smoothed `set_param`. Its
    effect is **latent anyway** (no hi-Z sources exist until Epic 5), so it would be recompile plumbing for
    zero audible payoff now.
  - **48V/phantom** is architecturally **inert on a preamp**: the existing `CondenserMic` self-emits
    phantom when its *own* flag is set (phantom flows upstream through the pull-based DAG and is
    approximated at the mic), so a preamp-side switch has **no path to reach the mic**. Real phantom supply
    needs an upstream side-graph (Epic-5 work, like the planned ground/clock side-graphs).
  - **AIR** (analog high-shelf) needs a new analog filter; **PAD** (in-`process` attenuation) *is* cleanly
    modelable now but adds little without a hot source.

  All four ride in with **Epic 5 (5.1)** when the preamp gets real physics — the switches appear when they
  are honest. _Rejected: a `MicPreamp` node carrying declared-but-inert INST/48V params now_ — a code check
  showed both would be cosmetic-or-latent, exactly what the ethos forbids. Recorded in `IMPROVEMENTS.md`.
- **`scarlett_8i6` catalog entry — minimal but honest.** Multi-node chassis reusing existing nodes: 2×
  `GainStage` preamps (gain + power) → 2× `AdConverter` (the digital "USB send"); a digital "USB return" → `DaConverter` →
  monitor + phones gain stages → analog outs; MIDI in/out via `EventThru`. Exposed face: **front** = 2
  combo inputs + a headphone out; **back** = line outs, digital send/return, MIDI, power. _Known
  simplifications (not bugs):_ USB is modeled as separate per-lane digital ports (our connector model is
  mono-per-lane; one-connector-many-channels is a 5.6 fidelity case), and **S/PDIF is deferred** (only
  real ports get jacks).
- **Focus via the registry (generalized — decision 2a).** The registry replaces the hardcoded `typeId ===
  "synth_voice"` (Screen) and `surface === "console"` branches in **both** render sites (in-world `item`
  and the focus overlay); a custom faceplate renders in the focus overlay too. Focusability = **has a
  registered focus surface** OR **is playable** (an events input ⇒ keybed, still derived — retiring
  `focus.ts`'s hardcoded `FocusSurface` kinds); `Console` becomes `channel_strip`'s registered focus
  component and the synth's ADSR `Screen` its registered embellishment. The keybed is still appended for
  instruments and the global-keyboard retirement (Story 4.8) is preserved. A larger bespoke focus surface
  (a DAW/touch display) is now expressible but **built only when a device needs it**.
- **Brand identity.** `skin.ts` gains an `accent`/`chassis` colour; the 8i6 faceplate reads it for its
  border/chassis via scoped CSS + tokens, and the **top-down floor-plan tile** (App's `item` snippet,
  `.plan-tile`, currently plain `--ae-bg-chip`) reads it too — so the red chassis shows from above. One
  value, both views.
- **Consistency guardrails (against N bespoke snowflakes).** Shared widgets + a small set of **layout
  primitives** (`Section`/`Legend`/`ButtonCluster`/`Silkscreen`, extracted while building the 8i6) +
  token-only colours/type. A **Vitest mount test per registered faceplate** asserts it references only
  **valid ids** and **places every param/port** — the web mirror of the Rust
  `catalog_aligns_with_exposed_face` guard.

- **Task 5.7.1 — `scarlett_8i6` catalog entry (`devices`).** Multi-node entry (2× `GainStage` preamps →
  2× AD; digital return → DA → monitor/phones gains; MIDI `EventThru`) with UI metadata
  (labels/kinds/connectors) positionally aligned to the exposed face. _Done:_ `catalog_aligns_with_exposed_face`
  + `descriptors_carry_engine_truth` pass; an `instantiate` test pins the multi-node port/param remap (as
  `channel_strip` does); the descriptor serializes camelCase; the device renders (via the **generic
  Panel**, pre-faceplate) in-browser.
- **Task 5.7.2 — Faceplate plumbing + `Chassis` + `Panel` refactor.** Add the `DeviceHandle` context, the
  `Chassis` primitive (bezel + flip + sets context), and the `Control`/`Socket`/`Reading` bound wrappers;
  **rebuild the generic `Panel` on `Chassis`** using the wrappers. _Done:_ every existing device renders
  and flips **identically** (fallback parity), knobs/faders/switches/jacks still drive the engine and
  patch; `pnpm check`/`typecheck`/`build` green; verified in-browser.
- **Task 5.7.3 — Device-UI registry + both render sites + focus generalization.** `device-ui.ts` registry
  (`typeId → component`, else `Panel`); wire it into the in-world `item` snippet **and** the focus overlay,
  replacing the hardcoded synth-Screen and console branches; rework `focus.ts` so focusability =
  registered-focus-surface ∨ playable, with `Console` and the synth `Screen` registered. Keybed still
  appended for instruments. _Done:_ synth (in-world + focus + keybed) and `channel_strip` (Console focus)
  behave as before, now via the registry; no hardcoded `typeId`/`surface` branches remain in App's render
  sites; in-browser parity.
- **Task 5.7.4 — `Scarlett8i6.svelte` faceplate + brand + primitives + guardrail.** The bespoke component:
  **front** (2 combo inputs with gain knobs, monitor + phones knobs, headphone jack, plus the power
  switch(es) the exposed face carries) and **back** (line outs, digital send/return, MIDI, power) laid out
  in scoped CSS; red chassis via a new `skin.accent`, threaded to the faceplate border **and** the
  top-down `.plan-tile`. Extract the `Section`/`Legend`/`ButtonCluster`/`Silkscreen` primitives and any new
  widget (a labeled toggle button / indicator LED) it needs. Add the **Vitest mount-test guardrail**
  (valid-ids + full param/port coverage per registered faceplate). _Done:_ zoomed in, the 8i6 reads as a
  simplified Focusrite — mixed front/back I/O, red chassis (in elevation **and** top view), section legends
  — its controls drive the live engine and its jacks patch with correct cable anchors; the mount test
  passes; full Rust gate + web `check`/`typecheck`/`test`/`build` green; verified in-browser by eye.

_Validate:_ un-authored devices render/flip **identically** through the generic `Panel` fallback (rebuilt
on `Chassis`); the **Scarlett 8i6** renders as a bespoke faceplate with **mixed-face I/O**, **red chassis**
in both the wall elevation and the top-down plan, and **section legends**, its controls driving the live
engine and its jacks patching with correct cable anchors; the preamps expose only **gain + power** (part 1;
INST/AIR/PAD/48V arrive in task **5.7.6**); the
**focus overlay** renders custom faceplates (and `Console` for
`channel_strip`) via the registry with the synth keybed intact; the **mount-test guardrail** proves every
registered faceplate references only valid ids and places all params/ports; the **layer rule holds** (no
layout vocabulary on the Rust descriptor); the full Rust gate (`cargo fmt --check && cargo lint && cargo
test && cargo wasm && cargo docs`) plus web `check`/`typecheck`/`test`/`build` pass; verified in-browser.

_Design notes — extended scope (part 2, folded in after 5.7.1–5.7.4 landed):_ The proving 8i6 is
deliberately reduced, and building it surfaced engine concepts missing to make it — or any faithful
interface — real. These are folded in as the tasks below rather than scattered to later stories (they were
first captured in `docs/IMPROVEMENTS.md`, now promoted here). Each carries its own rationale + done-state;
the larger ones (multichannel digital, routing, preamp physics) may be split into sub-tasks by the executor.

- **Task 5.7.5 — Device-level power gate (framework, not per-node).** Today each `GainStage` carries its own
  `powered` param, so a multi-node device exposes one power switch *per stage* — the 8i6 already shows 4,
  and adding line-I/O stages (5.7.8) would multiply them. Introduce a **device-level power** the faceplate
  presents once (a real interface is bus-powered — a single state), the per-node gates becoming an
  implementation detail driven by it (or one framework-level gate applied at the device boundary). This is
  the "generic framework-level power" deferral first noted in Epic 4 / Story 4.2. _Done:_ the 8i6 presents a
  single power control that silences the whole device (de-clicked, no recompile); single-node devices'
  power still works; catalog-alignment + engine tests green.
- **Task 5.7.6 — Preamp physics: INST / PAD / AIR / 48V.** The honest backing for the 8i6's front-panel
  switches (omitted in part 1 because none was modelable in today's engine). Each needs real work, in
  rough order of ease:
  - **PAD** — input attenuation: a smoothed in-`process` multiply (like `powered`), audible immediately.
  - **INST / hi-Z** — switches the preamp's **input impedance**. The loading divider is **baked at
    compile** (`schedule.rs`, from the port's static `InputZ`), so this needs either a recompile-on-toggle
    (structural, like repatching) or a runtime-re-solvable divider driven by a param. Audible payoff is
    latent until Epic-5 hi-Z sources exist, but the impedance change is real. Oracle: line-Z vs inst-Z
    divider loss against a constructed high-output-impedance source (§9).
  - **AIR** — an analog **high-shelf** filter (new analog DSP; EQ is digital-only today).
  - **48V phantom** — the hard one: `CondenserMic` self-emits phantom (it flows upstream through the
    pull-based DAG, approximated at the mic), so a preamp switch has **no path to reach the mic**. Real
    phantom needs a small **upstream phantom-supply side-graph** (mirroring the planned ground/clock
    side-graphs) — which is an Epic-5 decision-level piece; **48V may stay deferred** if that side-graph
    isn't built here, while INST/PAD/AIR land. _Done:_ the preamp exposes gain + the modeled switches with
    hand-calc oracles where analog; the 8i6 faceplate shows them; engine gate green.
- **Task 5.7.7 — Multichannel digital ports.** Every digital port today is a single mono lane
  (`lane_count() == 1`); a real USB (or ADAT/S-PDIF) connector carries **many channels bidirectionally over
  one physical connector**. Add a **multichannel digital port/lane** concept (a port with N lanes behind one
  jack) so an interface's USB is one connector, not the several mono digital ports the 8i6 fakes now. This
  is the Epic-1-deferred "multichannel digital ports (ADAT 8-lane etc.)" item — large; touches the port/lane
  model, `compile`, and the wasm/UI descriptor. _Done:_ a device declares a multichannel digital port; the
  8i6's USB becomes one; existing mono digital paths unchanged; engine + `cargo wasm` gate green.
- **Task 5.7.8 — 8i6 full analog I/O + S/PDIF.** With device power (5.7.5) and multichannel digital (5.7.7)
  in place, grow the reduced 8i6 to the real unit: add the **rear line inputs** and the **additional line
  outputs** (so it reads as ~8-in/6-out, not 2-in/1-line-out) and **S/PDIF** in/out (deferred in part 1).
  Extend the catalog entry (more AD/DA/gain nodes) and place the new I/O on the faceplate (front vs back per
  the real panel). _Done:_ the 8i6's exposed face matches the real unit's I/O count; catalog-alignment +
  `instantiate` remap tests updated; the faceplate places everything (guardrail green); in-browser.
  - _Also delivered here — the `computer` peer + round-trip latency._ Shipped a minimal `computer` USB peer
    (8-lane send in, 6-lane return out; per-lane send meters; an 8→6 loopback `Matrix`, default send 1→
    return 1). Closing the monitoring loop through it (8i6 → computer → 8i6 → speaker) is a graph cycle in
    the delay-free DAG engine, so added a **delayed-edge** primitive: `Graph::connect_delayed` marks an edge
    that is **cut from the topological sort** and served from the persistent output pool (its pre-loop copy
    reads last block's value), giving exactly **one block of round-trip latency** — physically the DAW's
    playback trailing its input. The `computer` declares its USB output a latency source
    (`CatalogEntry.delayed_outputs`); `build_patch` wires such edges delayed. **The schedule stays a strict
    DAG** — feedback is expressed as bounded latency, not a same-block solve. The default scene is now this
    playable loop. Oracles: a delayed two-node loop compiles (an undelayed one still errors), the delay is
    exactly one block, and the full 8i6↔computer loop builds and is audible.
- **Task 5.7.9 — Runtime routing matrix (engine concept + focus-view UI).** Interfaces and mixers route any
  input to any output through an internal **matrix** (the 8i6's is Focusrite Control). We have **no
  runtime-configurable routing** — internal wiring is fixed `InternalEdge`s and inter-device signal is fixed
  graph edges. Add a **routing abstraction**: per the routing-seam note in `crates/devices/src/catalog.rs`,
  runtime-switchable routing "is **not** a topology change — it lives inside a node behind a control param",
  so a **params-driven matrix node** (route input *i* → output *j* via matrix params, no recompile) is the
  intended shape (vs. user-repatching, which recompiles). Surface it in the **focus view** as a matrix grid
  (rows = inputs, cols = outputs), registered as the interface's focus surface (the part-1 registry already
  supports per-device focus surfaces). _Done:_ a routable node routes inputs → outputs at runtime via matrix
  params; a focus-view matrix grid drives them; engine + web gate green; in-browser.
- **Task 5.7.10 — Device dimensions pass.** The catalog `FormFactor` boxes are rough guesses (the 8i6 was
  authored 210×50×150 mm; several devices likely approximate). Do a pass against real gear so the spatial
  world's relative sizes read right. Small. _Done:_ dimensions reviewed/corrected; `catalog_carries_sane_form_factors`
  green; the spatial layout looks right in-browser.

_Validate (part 2):_ the 8i6 is a **faithful** interface — full analog + digital I/O (multichannel USB,
S/PDIF, rear line ins, line outs), a **single power** control (not per-stage), honest **INST/PAD/AIR** (and
48V if the phantom side-graph lands here), and a **routing matrix** in its focus view assigning inputs to
outputs at runtime; a `computer` USB peer completes the **playable monitoring loop** — sound travels
mic/synth → preamp → AD → USB → computer → USB return → DA → monitor, closed through a **delayed edge**
(one block of round-trip latency; the schedule stays a DAG); device **dimensions** corrected; the full Rust
gate (`cargo fmt --check && cargo lint && cargo test && cargo wasm && cargo docs`) plus web
`check`/`typecheck`/`test`/`build` pass; verified in-browser.

_Decision — ground-loop hum should become emergent from grounding topology (deferred to this Epic)._
Today (Story 1.5) `Cable::with_hum` is a **manual** injection — the user asserts "a ground loop exists
on this cable." That's a phenomenological stand-in, not the final design. A ground loop is a **loop in
the ground network**: two mains-earthed devices _also_ tied together by a cable shield form two ground
paths between them ⇒ circulating 50/60 Hz current ⇒ hum. Break any leg (a floating/battery device, a
**ground lift**, transformer/DI isolation) and the loop — and the hum — is gone, _regardless_ of
balanced vs. unbalanced (balanced merely rejects the hum when a loop does exist; it doesn't prevent the
loop). So whether hum _appears_ is a property of the patch's grounding, and should **emerge**, not be a
flag:

- Model a small **ground-connectivity** side-graph — devices declare mains-earthing; cables declare
  whether the shield bonds the two grounds and whether it's lifted at an end.
- At **compile**, **detect cycles** in that graph; a cable on a cycle between earthed devices is in a
  ground loop ⇒ inject hum there. A lift / floating device / isolator removes an edge ⇒ no cycle ⇒ no hum.
- This is compile-time **connectivity analysis, not a per-sample electrical loop solve**, so it honors
  the "local solve only / no global nodal solve / signal graph is a DAG" decision (§5.3) — same kind of
  cheap graph pass we already run for signal-DAG cycle detection, just on a separate graph.
- The hum **amplitude stays phenomenological** (the induced voltage from loop area / earth-potential is
  the "EM source" we hold out of scope). Only the _appearance and location_ become emergent.
  _Prerequisites (none exist yet):_ a ground/earth concept on devices, shield modeling on cables, and
  ground-lift controls — naturally introduced alongside Story 5.1 (patchbay/wiring) and consumed by the
  "fix the hum" diagnostic here. ROI is high then (the heart of the troubleshooting lesson), low now.

_Decision — clock domains and their failures emerge from a clock-distribution side-graph + real
per-domain rates (deferred to this Epic)._ Through Story 1.6 there is a single internal clock domain
and no async boundary, so a `SampleBuffer` merely carries its producing oscillator's identity and
rate. The full model lands here, mirroring the ground-loop-hum approach (a cheap compile-time
connectivity pass over a side-graph, plus an emergent runtime consequence — never a flag):

- Devices declare a **clock source** — `Internal(rate)`, `RecoverFrom(digital input)`, or
  `WordClock(input)` — and word-clock links form a **clock-distribution side-graph**, independent of
  the audio DAG (a dedicated master is a star over BNC, decoupling clock topology from audio routing:
  re-patch audio without breaking sync, one place to change rate, no clock loops).
- At **compile**, resolve the side-graph: assign each device to a **clock domain** (follow sources to
  a root master), detect no-clock / clock-loops / rate conflicts, and mark the **async boundaries**
  where two domains meet.
- At **runtime**, the consequence is **emergent**: each domain advances a phase accumulator at its
  real rate (with crystal-ppm differences) against the analog continuum, and a finite **elastic FIFO**
  at each async boundary genuinely over/underflows → the clicks/slips of an unlocked link. Sharing a
  master collapses the domains (no boundary, no slip); a **sample-rate converter** at the boundary
  re-grids one domain onto the other (the honest fix).
- **Out of scope:** the physical layer — line coding (biphase-mark), PLL clock recovery, bit
  de-framing (inside-the-box circuitry, §2). We model whether a link _locks_ and _slips_, not its
  bitstream. True jitter _spectra_ are a further optional depth we do not expect to need.
  _Prerequisites:_ the carrier/clock seam and `ClockDomainId` stamp (Story 1.6); multiple digital
  devices and the fractional resampler (this Epic). ROI is high here (multi-device digital sync is the
  heart of the lesson), nil before.

### Story 5.8 — 48V Phantom Power — ✅ **Complete**

_Goal:_ Make +48 V phantom power **honest end-to-end**: a patchable **condenser mic** whose power
arrives from the device it's plugged into, a **balanced preamp front-end** that separates the phantom
pedestal from the audio the way real hardware does, and a **48V switch on the 8i6** that actually
reaches the mic. Retires the last in-domain label on the signal path — `CondenserMic`'s self-asserted
`powered` flag (condenser.rs's documented "informed approximation") — by replacing it with a
**compile-time DC operating-point solve** over the patch's real connectivity. Anchors to
PROJECT_PLAN §2 (phantom emerges from voltage physics, never a flag), §5.3 (local solve at compile /
per-sample forward), and the Epic-4/5 layer rule (catalog owns specs, web owns look & feel). Closes the
one switch Story 5.7.6 left deferred ("48V may stay deferred if that side-graph isn't built here").

_Watch out:_

- **The DC solve is compile-time physics, not a label.** The interconnect is linear (nonlinearity lives
  in devices) and the phantom network is static (48 V behind fixed resistors), so **superposition**
  splits the problem: solve the DC bias network once at compile (the operating point — SPICE `.OP`),
  superpose the AC signal per-sample forward as today (`osku_physics_concepts.md` §17). Do **not** try
  to push per-sample voltage backwards through the pull DAG — the stationary DC axis is exactly what
  compile-time solving is for, the same trade the loading divider already makes.
- **Common-mode rejection cannot be applied after a nonlinearity.** The 48 V pedestal must be removed
  **before** the preamp's gain/rail clamp (`clamp(48·g, ±10)` on both legs pins the rail and
  annihilates the differential — §17). Difference-first is *exact* at the ideal-CMRR altitude.
- **Hot-path contracts hold:** the resolution pass allocates/fails only in `compile`; `process` stays
  alloc-free, panic-free, branch-light. The capsule tone goes through the seeded/deterministic
  machinery (no ambient entropy; a phase accumulator, not `sin` of wall-clock).
- **Don't break the default scene:** the synth (unbalanced) feeds the 8i6 preamp today. Making the
  preamp balanced requires the unbal→bal edge rule to land **first**, and a regression oracle that the
  synth→preamp loading gain is *numerically unchanged*.
- **Scope guards:** single-edge resolution only (mic directly into the supplying input); no finite
  CMRR; no per-sample supply modulation; no back-driven DC into non-phantom sources (48V engaged with a
  synth plugged in simply resolves nothing — the real-world "mostly harmless" case, simplified).

_Design notes (settled at planning):_

- **48V is a structural config (recompile-on-toggle), like INST.** The DC network *is* topology; when
  it changes, recompile — the same pattern as repatching and the INST impedance switch, riding the
  existing `ConfigDescriptor`/schedule-swap machinery. Acoustically safe: the pedestal cancels at every
  balanced receiver in both states, so the swap can't click. _Rejected: a runtime 48V param with a
  compile-baked cross-node "supply feed" scaling the mic's pedestal through the supplier's smoother_ —
  it buys the seconds-long RC charge-up ramp (mic audibly fades in) but needs a new cross-node param
  mechanism; recorded as a possible later upgrade, not this story.
- **Resolution lives in engine `compile`, declared via `Node`-trait hooks.** Nodes declare phantom
  roles per port — a supply (`48 V` behind `6.8 kΩ` per leg, engaged or not) and a load (a DC
  resistance + minimum operating volts). These are **circuit-topology declarations**, the same class of
  port-fact as `InputZ`/`OutputZ` — not labels on the signal. `compile` walks analog edges; where an
  engaged supply faces a declared load it solves the DC divider (reusing the §5.3 local-solve
  primitives) and hands the producer its terminal volts via a `prepare`-like hook (default no-op).
  _Rejected: resolving in `build_patch`_ — less engine churn but puts electrical physics in the product
  layer, against the layer rule.
- **Sag and dead-mic emerge from the solve.** Supply = 48 V behind 2×6.8 kΩ (3.4 kΩ effective for
  common-mode current) + the connection's cable series R + the mic's DC load (constant-resistance
  model). Hand calc at a ~3 mA-class load (12.7 kΩ), no cable: `48·12 700/(3 400+12 700) = 37.86 V`.
  The mic runs iff terminal volts ≥ its declared minimum — a threshold in the mic's own electronics,
  the same species as rail clipping, not a flag. _Rejected: constant-current load_ — makes the solve
  nonlinear (iterative) for no audible payoff; constant-R keeps it a divider.
- **Unbalanced-into-balanced is an edge rule, not an adapter node.** A 1-conductor output into a
  2-conductor analog input becomes legal in `compile`: the hot lane gets the normal divider transform,
  the cold lane is **grounded (0 V)** — literally what a TS plug in a combo jack does (sleeve shorts
  the cold pin). The differential divider formula is unchanged (`Zin/(Zout+Rc+Zin)`), so existing
  unbalanced sources keep their exact gain. _Rejected: a build-inserted adapter node_ — electrically
  wrong: its own Z faces split one loading divider into two near-unity ones, silently deleting the
  loading loss.
- **`MicPreamp` grows a balanced front-end: difference-first, then the existing chain.** Input becomes
  `InputZ::balanced(...)` (2 conductors); `process` computes `s = V+ − V−` (exact pedestal/hum
  cancellation at ideal CMRR), then PAD → gain → rail → AIR → power as today (2-in/1-out node shape:
  the `BalancedReceiver` precedent). The per-leg coupling-caps topology becomes distinguishable only
  under finite CMRR — deferred with it. The declared `InputZ` lumps the phantom feed network's AC
  loading, as `InputZ` already lumps everything.
- **The capsule is a declared boundary stand-in: a deterministic test tone.** Acoustics are out of
  scope (PROJECT_PLAN §2), so `CondenserMic`'s capsule emits an internal sine (level + frequency
  params, mic-level ~10 mV default, phase-accumulator deterministic), riding the *resolved* pedestal:
  `V± = V_dc ± s/2`. Toggling 48V audibly kills/raises the tone — the story's in-browser payoff.
- **Deferred — the "air link" story (separate, not this one):** the missing abstraction from a
  vibrating source (speaker cone, vocal cords, string) over air to the capsule. Not accurate
  acoustics — a simple "analog wave over the air" carrier with a transduction seam at each end
  (pressure↔volts is the declared boundary). Noted at planning: acoustic feedback (mic hears speaker)
  is already expressible via the delayed-edge primitive — one block ≈ 2.7 ms ≈ 0.93 m of air at
  343 m/s, so the forced latency is nearly physical — and the open question is how an air path is
  "patched" (proximity/geometry, not a cable). Howlround as an emergent challenge is the payoff.
- **Known simplifications (not bugs):** single-edge phantom resolution (through-a-patchbay deferred
  with 5.1); fan-out from one mic into two supplying inputs unresolved (deferred, real-world-weird);
  no DC back-drive onto non-load sources; the ramp-up transient (structural toggle is instant).

- **Task 5.8.1 — Unbal→bal edge rule (`engine::compile`).** Make a 1-conductor analog output into a
  2-conductor analog input legal: hot lane = the normal baked divider transform, cold lane = 0 V
  (TS-in-combo physics). Conductor inference, pool sizing, and interference coupling (pickup/hum still
  lands on both conductors) updated. _Done:_ oracle — the divider gain equals the 1→1 case exactly
  (hand calc in comment); a driven balanced receiver recovers the full signal (`s − 0 = s`); existing
  1→1 and 2→2 paths byte-identical; engine gate green.
- **Task 5.8.2 — `MicPreamp` balanced front-end.** Input face → `InputZ::balanced`; `process` takes
  `V+ − V−` first, then the existing PAD/gain/rail/AIR/power chain. The 8i6 catalog entry keeps
  working via 5.8.1 (synth still plugs in). _Done:_ oracles — a `48.005/47.995` pair comes out as
  `gain·0.01` with **zero** DC (pedestal rejected before the clamp); common-mode hum injected on both
  legs cancels; the default scene's synth→preamp gain is numerically unchanged (regression hand calc);
  engine + devices tests green.
- **Task 5.8.3 — Phantom declarations + the compile-time DC solve.** `Node`-trait hooks for supply
  (volts, per-leg feed R, engaged) and load (DC resistance, minimum volts); `compile` resolves each
  analog edge's operating point and delivers terminal volts to the producer via a default-no-op hook.
  `CondenserMic` consumes it: pedestal = resolved volts, dead below its minimum — **the `powered`
  flag is deleted**. `MicPreamp` grows the supply declaration (engaged via constructor ← config).
  _Done:_ oracles — `48·12 700/16 100 = 37.86 V` (no cable), sag grows with cable R (hand calc), a
  long/lossy enough run drops below the minimum ⇒ silent mic, no supply (or disengaged) ⇒ 0 V ⇒
  silent, supply facing a non-load ⇒ no-op; engine gate green.
- **Task 5.8.4 — Capsule test tone.** `CondenserMic`'s capsule emits a deterministic internal sine
  (smoothed level + frequency params, ~10 mV default) differentially on the resolved pedestal:
  `V± = V_dc ± s/2`. _Done:_ oracles — output common-mode = `V_dc` exactly, differential amplitude =
  the level param; deterministic across runs (same seed ⇒ identical buffers); no-alloc test still
  green.
- **Task 5.8.5 — `condenser_mic` catalog device + the 8i6 48V config.** Catalog entry (balanced XLR
  out, level/freq params, phantom-load declaration, sane `FormFactor`); the 8i6 gains a **48V**
  `ConfigDescriptor` (one switch, both preamps — matching the real unit) mapped to the preamps'
  supply-engaged constructor arg. _Done:_ `catalog_aligns_with_exposed_face` +
  `descriptors_carry_engine_truth` + an `instantiate` remap test cover the mic; toggling the 8i6's
  48V config rebuilds with supplies engaged; devices tests green.
- **Task 5.8.6 — Web: 48V switch + mic in the world.** The 8i6 faceplate presents the 48V toggle (the
  INST config-toggle pattern; button + LED); the mic renders via the generic `Panel` fallback (no
  bespoke faceplate — it earns one later); an XLR cable patches mic → combo 1. _Done:_ in-browser —
  48V off ⇒ silence, on ⇒ the capsule tone through preamp → AD → computer loop → monitors; faceplate
  mount-test guardrail green; `pnpm run format` + web `check`/`typecheck`/`test`/`build` green.

_Validate:_ a condenser mic patched into the 8i6's combo input is **silent until the 8i6's 48V switch
is engaged** and audible through the full monitoring loop after; the pedestal is genuinely present on
the wire (common-mode `V_dc`) and **cancels at the balanced front-end before any nonlinearity**; sag
emerges from the DC divider with hand-calc oracles (`37.86 V` at the reference load; below-minimum ⇒
dead mic); the synth still plugs into the same combo jack with **numerically unchanged** gain; the
mic's self-`powered` flag is gone from the engine; the full Rust gate (`cargo fmt --check && cargo
lint && cargo test && cargo wasm && cargo docs`) plus web `check`/`typecheck`/`test`/`build` pass;
verified in-browser.

*Delivered:* all six tasks as planned (one commit each), Validate met in-browser (mic silent →
48V on → capsule tone through the monitoring loop → off → silent). Deviations from plan: none
structural; the notable in-task design choices, for future reference:

- **5.8.1 (grounding edge):** the cold leg is an ordinary `EdgeTransform` with `gain = 0` (no new
  `EdgeKind` variant, no second hot-path arm), its source index clamped to the hot lane so nothing
  can index out of bounds; step-7b interference already cloned onto every conductor, so
  common-mode pickup/hum on the grounding edge came free. Matched edges bake byte-identically.
- **5.8.2 (balanced front-end):** `MicPreamp::new` kept its `z_in: InputZ` signature with a
  construction-time balanced assert (the rail-check precedent) rather than switching to bare
  `Ohms` — honest about the declared face, no call-site ripple.
- **5.8.3 (DC solve):** declarations landed as `PhantomSupply`/`PhantomLoad` in
  `electrical/phantom.rs` with the solve as `PhantomSupply::terminal_volts` **reusing
  `divider_gain`** (compile stays a thin walk); `MicPreamp` grew a chainable
  `with_phantom(engaged)` builder (default off) instead of a positional bool; the fan-in guard
  (`CompileError::PhantomFanIn`) counts only *engaged* supplies.
- **5.8.4 (capsule tone):** the oscillator **free-runs while dead** (power folds into a per-block
  gate multiplier — no per-sample branch, phase continuity on re-power); unprepared ⇒ phase step 0.
  Noted at review: when the air-link story lands, **FREQ dies with the internal sine** while
  **LEVEL mutates into capsule sensitivity** (the pressure→volts mV/Pa figure, likely a catalog
  spec rather than a knob).
- **5.8.5 (catalog):** the both-preamps-from-one-key test reads the existing input-meter Peak
  readouts (one build proves both channels); devices-crate tests kept the house abs-diff style
  (no new `approx` dev-dep).
- **5.8.6 (web):** a new mm-scaled `ConfigButton` hardware widget (the rem-scaled `ConfigSwitch`
  stays a focus-surface aesthetic) with a **red** engaged lamp (phantom hardware convention); the
  interactive 48V lives on the faceplate only (real front-panel button), trivially addable to the
  Focusrite Control surface later; the mic renders via the generic `Panel` fallback with a
  one-line skin. Field note: a config widget whose key misses the descriptor renders nothing —
  a stale WASM artifact thus shows as a zero-width div; a dev-mode warning in that branch is a
  candidate hardening (IMPROVEMENTS.md).

### Story 5.9 (part 1) — Duplex digital transport & topology-derived delay — ✅ **Delivered**

_The broader Story 5.9 is a **simplified OSI-Layer-2** link/forwarding layer beneath Story 5.2's
IP/subscription (Dante/AES67) layer. **Part 1** — bidirectional single-connector links + automatic
latency insertion — is delivered here; the L2 forwarding proper (addressed flows, switches, clock
domains) stays deferred to 5.2/5.3 (see "Deferred" below, where the idea-gathering is kept intact)._

_Goal:_ Digital connections were unidirectional (one `OutputPort` → one `InputPort`, an
`EdgeKind::DigitalRoute` sample copy), so a duplex link (USB-C, Ethernet) had to be authored as two
separate one-way cables. Make one physical connector carry **both directions** (the 8i6/computer USB as
one USB-C jack), and make digital feedback loops **robust** — a monitoring loop through a duplex link
must build and sound without the author hand-placing a delayed edge.

_Calibration — most of this was already right:_ per-strand fidelity is correctly **absent** (a digital
edge is an ideal lossless copy — no `Lifted`/per-conductor one-pole/common-mode; that machinery is
analog-only); multichannel-behind-one-connector already existed (`AudioFormat.channels`,
`DigitalMux`/`DigitalDemux`, the 8i6's 8-ch send + 6-ch return). The **only** trait digital inherited
from the analog cable was the **unidirectional edge** — and that is the **DAG invariant** (§5), not a
leftover: a wire carrying live data both ways in one block is a cycle. So a duplex link is an
**authoring-layer** construct that lowers to two engine edges; the engine datapath stays unidirectional.

_Design decisions (settled before building):_

- **#2 delay is topology-derived, in `compile` (keep the hint).** `compile` auto-breaks any residual
  **digital** cycle by delaying the lowest-index digital edge on it (deterministic; a delayed edge is an
  ideal digital copy carrying one block of round-trip latency, physical only on a buffered digital link).
  An all-analog cycle has nowhere to carry the latency and still rejects as `CompileError::Cycle`.
  `delayed_outputs` stays a **hint**: a build-declared latent output (the DAW/computer) is pre-marked
  `delayed` and excluded, so `compile` only breaks what the author didn't — and the block of latency
  lands on the physically-correct leg (playback trails input). _Rejected: pure topology (drop
  `delayed_outputs`)_ — arbitrary latency placement within a loop. _Rejected: keep it device-declared_ —
  fragile to topology the author didn't foresee (a duplex link between two non-latent devices never
  breaks).
- **#1 fidelity: one duplex jack.** A device declares an `(output, input)` pair as one connector; the web
  draws one USB-C jack and one cable; the engine still sees two edges.

_Shipped:_

- **#2 (engine).** `topo::reaches` (compile-time reachability); an auto-break loop in `compile` (step 5c)
  that delays a digital edge on each residual cycle; `CompileError::Cycle` re-documented as the
  unbreakable (analog-feedback) case. Oracles: a digital 2-cycle of ideal edges auto-compiles; an
  auto-broken digital self-loop integrates one block/block (identical to a manual delayed edge); an
  all-analog cycle still errors; the computer monitoring loop's latency still lands on the playback leg.
- **#1 (Rust core).** `CatalogEntry.duplex_links: &[(out_id, in_id)]` (8i6 `(0,7)`, computer `(0,0)`) →
  `PortDescriptor.duplex_partner` (JS `duplexPartner?`, `skip_serializing_if` so one-way jacks omit it);
  `Connection.duplex: bool`; `build_patch` expands a duplex connection into **both** directed edges (the
  reverse via each jack's partner), erroring `BuildError::NotDuplex` on a non-duplex port; connection-loss
  accounting stays 1:1 with scene connections (the digital reverse leg is untracked). Oracle: one duplex
  USB connection builds the full mic→8i6→computer→8i6→speaker loop and is audible; a duplex flag on a
  one-way jack errors.
- **#1 (web).** `DuplexSocket.svelte` renders the 8i6/computer USB as one jack; `Jack` carries a
  `data-jack-alt` for the partner leg and `jack-anchors` registers it at the same centre, so a cable
  anchors either direction at the one jack; `evaluateConnection` joins two duplex jacks into a
  `duplex:true` connection and **skips the feedback-loop check** (a duplex link is a cycle by design);
  `patching` carries `duplexPartner` through; the faceplate guardrail credits a `DuplexSocket` as placing
  both its ports.
- **Follow-up bug fixed in-story.** `commitCable` rebuilt the stored `Connection` from `from`/`to` only,
  **silently dropping `duplex`** → every duplex cable committed one-way (no return leg → silent). Fixed to
  spread the whole verdict connection; regression test in `scene-ops.test.ts`. Found via a user-reported
  no-sound patch; the engine/build side was correct from the first commit — the symptom was UI
  persistence.

_Validate:_ the 8i6 and computer each show **one** USB-C jack; plugging **one** cable authors a `duplex`
connection that `build_patch` expands to both edges, and the mic monitoring loop closes and sounds; a
UI-authored digital 2-cycle auto-compiles while an analog cycle still errors; the full Rust gate (`cargo
fmt --check && cargo lint && cargo test && cargo wasm && cargo docs`) plus web `check`/`typecheck`/`test`
pass; verified in-browser (delete + re-plug the USB-C cable → sound).

*Delivered:* #1 + #2 across five commits on `e5-s9/duplex-digital-transport`; Validate met in-browser.
Notable finding: the one-way-cable symptom was a UI persistence bug (`commitCable` dropping `duplex`),
not an engine/build defect — the duplex expansion + auto-break were correct throughout.

_Deferred to 5.2 / 5.3 (the rest of the L2 vision — idea-gathering kept intact):_

- **(c) Addressed flows — the net-new carrier concept.** The lane model is positional (source lane _k_ →
  dest lane _k_), right for interfaces/ADAT/USB (a fixed compile-time bundle). An L2 **switch** forwards by
  **address**, not position: an ingress flow carries a destination, the switch picks egress from a
  forwarding/subscription table. Nothing in `SampleBuffer`/`Lane` carries a flow identity. The heart of the
  networking work — decide whether it reuses `Sample` lanes + a routing table or gets its own addressed
  carrier.
- **(d) Channel-count equality is interface-only.** `build_patch`'s `ChannelCountMismatch` (equal counts)
  is right for a point-to-point cable, wrong for a switch (forwards a subset) and for per-direction duplex
  — it becomes a per-flow/per-direction check, not an edge invariant.
- **(e) Clock domains are the true prerequisite (5.3).** Two independently-clocked devices on one link are
  an async boundary; the return leg's latency is really elastic-FIFO slip, not a fixed block (today: one
  clock domain, cross-rate rejected as `ClockCrossingUnsupported`). The one-block delayed edge is a
  stand-in; "clock not locked → dropouts" depends on 5.3.
- **Design leanings for the switch (confirm at 5.2 planning):** subscription changes as
  **recompile-on-change** (structural, like INST/48V) to keep the DAG static; the within-device 14×14
  `Matrix` is the runtime-routing precedent for the between-device table; make consequences **emergent**
  (per-hop latency, bandwidth exhaustion → dropouts, missing subscription → silence, clock-not-locked);
  **skip** MAC/VLAN/QoS/frame encoding unless they produce an audible/measurable consequence (no flagging —
  §4/§9).

_Open decisions (non-blocking):_

- **(i)** `SCHEMA_VERSION` was **not** bumped for duplex, so a pre-duplex saved scene with a one-way USB
  link loads verbatim (silent monitoring). Left as-is because a one-way USB is a *legal* patch — decide
  whether to bump (force-discard stale benches) when it bites.
- **(ii)** the web's `wouldCreateCycle` still rejects a **one-way** digital loop authored by drag even
  though the engine (post-#2) would auto-break it; only duplex bypasses it today. Make the web cycle check
  domain-aware if UI-authored digital loops become a case.

### Story 5.10 — Dynamic computer I/O — ✅ **Done**

_Goal:_ The `computer` hardcodes the 8i6's USB shape — 8 sends / 6 returns baked into its catalog
entry as 11 nodes, 22 hand-wired internal edges, and statically authored meter/grid labels. A real
computer has no channel count of its own: it enumerates whatever the attached interface's driver
publishes. Make the computer **adapt to the attached interface**. The publication half already exists —
`PortDescriptor.channels` is engine truth derived from the port face's `lane_count()` — so this story
builds the consumption half: the computer's shape is **derived from the published face of what's
plugged in** (PROJECT_PLAN's derive-from-the-model rule; no parallel channel constant), with the
catalog owning specs and the web owning enumeration + look & feel (the Epic-4/5 layer rule). Payoff:
any future interface (a 2i2, a big rack unit) works against the same computer with no new peer device.

_Watch out:_

- **Hot-path contracts hold:** sized nodes allocate at construction only; `process` stays
  alloc-free/panic-free (per-lane loops, no per-N dispatch or growth).
- **Id stability is the trap.** The meter's per-lane readout ids must keep `(0, 1)` at n = 1 (the
  8i6's own meters address `PEAK_DBFS`/`RMS_DBFS`); matrix crosspoint ids are row-major `i·m + j`, so
  a change of M **reshuffles every id** — never remap saved params across a re-enumeration, reset
  them (design note below).
- **The "exposed face is config-independent" invariant is deliberately retired** (`instantiate`/
  `describe` docs, `descriptors()` built from `DeviceConfig::EMPTY`). Update the docs and the
  alignment-pinning tests to the new contract — don't work around them.
- **Don't touch the 8i6's own matrix face.** Its 14×14 `Matrix` genuinely needs mono ports (inputs
  arrive on separate wires from separate ADs/preamps); only the computer wants the lane-port variant.
- The type-level `descriptors()` catalog listing keeps working, showing the default (EMPTY-config)
  face.
- **Scope guards:** one USB port per computer (no hub); computer↔computer USB does **not** enumerate
  (both keep their configured/default shape — the equal-count check still guards the edge); per
  5.9's deferred (d), `ChannelCountMismatch` equality stays the right point-to-point rule and the
  backstop for hand-authored patches.

_Design notes (settled at planning):_

- **Multichannel-port nodes keep the topology shape fixed — no imperative-builder `CatalogEntry`.**
  The computer's node count only varied with N because meters/matrix speak mono ports (forcing the
  demux + N meter nodes + mux). With lane-aware nodes the entry is **two nodes and one internal
  edge** regardless of counts — meter-bank(N) → lane-matrix(N→M); the demux/mux disappear (they
  existed only to adapt lanes to mono ports). The static entry suffices: `NodeBuilder` already
  receives `&DeviceConfig`, so the builders construct sized nodes. _Rejected: the catalog module
  doc's anticipated "imperative-builder variant"_ — dynamic node/edge lists are heavier machinery
  than the problem needs; it stays available for a future genuinely-variable topology.
- **Channel counts are structural config (`usb_sends` / `usb_returns`), written by host-side
  enumeration.** When the UI connects/disconnects the USB duplex cable it reads the interface's
  published port `channels` and writes the computer instance's config (rebuilding via the existing
  config→recompile path). A loaded patch never enumerates — config serializes in the patch, so the
  IR stays self-describing. _Rejected: `build_patch` negotiation (sizing the computer from whatever
  is cabled to it)_ — breaks the compositional per-device expansion and makes a patch's meaning
  depend on inference rather than what's written.
- **Unattached default: 2×2 — the built-in sound card.** Realistic (a DAW with no interface shows
  the built-in device). Existing computer tests set 8×6 config explicitly; pre-story bench URLs that
  relied on the implicit 8×6 break (accepted, not a bug). _Rejected: keeping 8×6_ — the default
  would encode one specific interface.
- **Loopback default: diagonal over min(N, M)** (send k → return k) — the 8i6 matrix's own
  identity-default philosophy; every return carries signal out of the box. _Rejected: today's
  first-two-lanes-only loopback._
- **The config keys are hidden (host-driven).** Not declared in `configs`/`ConfigDescriptor` —
  undeclared keys already flow through the IR and `DeviceConfig::get_or` (no new `ConfigKind`
  widget needed); the faceplate displays the *detected* counts read-only, derived from the
  per-instance descriptor. You don't type your computer's channel count; you plug in an interface.
- **The descriptor becomes per-instance.** `describe` grows a config-aware path exported over wasm
  (`describe_device(type_id, configs)`); the type catalog keeps the default face. Readout labels
  ("Send k Peak/RMS") and the `GridSpec` row/col names are **generated** from the built face's
  counts — the same synthesis precedent as the grid's crosspoint labels.
- **Re-enumeration resets the matrix params to the loopback default.** Crosspoint ids reshuffle with
  M, so remapping saved `ParamSetting`s is wrong-headed — and a reset is what a real DAW does when
  the I/O device changes. Within one saved patch, config + params serialize together, so ids stay
  self-consistent.
- **Enumeration lives in web TS (`scene-ops`).** An authoring-layer act like
  `commitCable`/`disconnect`, sitting next to them with Vitest coverage; Rust's
  `ChannelCountMismatch` stays the backstop. _Rejected: a devices-crate helper over wasm_ — extra
  marshalling for ~10 lines of logic the TS layer already has the data for.

- **Task 5.10.1 — Engine: multichannel `DigitalMeter`.** Grow the meter to N lanes: one N-lane
  input, one N-lane exact-passthrough output, per-lane Peak/RMS readouts at ids `(2k, 2k+1)` — n = 1
  preserves today's face and the `PEAK_DBFS`/`RMS_DBFS` constants (keep `new` mono or grow an arg;
  call-site ripple is small either way). _Done:_ oracles — distinct per-lane constants read back as
  hand-calc'd dBFS (lanes at 1.0 / 0.5 / 0.25 → 0 / −6.02 / −12.04 dBFS peak, calc in comment; a
  full-scale sine's RMS = peak − 3.01 dB); n = 1 matches today's meter exactly; no-alloc test still
  green; engine gate green.
- **Task 5.10.2 — Engine: lane-port `Matrix`.** A construction variant whose face is one N-lane
  input / one M-lane output (instead of N + M mono ports), sharing the crosspoint-param and
  f64-summing core; crosspoint ids unchanged (row-major `i·m + j`). _Done:_ oracles — hand-calc'd
  gain-weighted sums across lanes match the mono-port matrix under the same gains; diagonal defaults
  route k → k; engine gate green.
- **Task 5.10.3 — Devices: the config-driven computer.** Rewrite the entry to
  meter-bank(N) → lane-matrix(N→M) with N/M from `usb_sends`/`usb_returns` (default 2×2, diagonal
  loopback); generated readout + grid labels sized from the built face; retire the
  config-independent-face invariant in the `instantiate`/`describe` docs; existing computer loop
  tests set 8×6 config explicitly. _Done:_ oracles — the default computer expands 2×2 (4 readouts,
  4 crosspoints, USB port channels 2/2); an 8×6-config computer reproduces today's playable-loop and
  duplex-cable tests unchanged; a mismatched hand-authored patch still errors
  `ChannelCountMismatch`; alignment-pinning tests updated; devices gate green.
  **Note (pulled forward from 5.10.4):** `build_patch`'s pre-compile channel-count check read the
  config-blind *type* descriptor (computer always 2×2), so an 8×6 loop was wrongly rejected. Fixed by
  making `describe` config-aware and adding the `describe_device(type_id, config)` seam, then keying
  `build_patch`'s `descs` by scene id from each device's config (dropping the `types` indirection). So
  the Rust-side config-aware descriptor already exists and is tested here.
- **Task 5.10.4 — wasm: export the per-instance descriptor.** The config-aware `describe` +
  `describe_device(type_id, config)` seam **already landed in 5.10.3** (build_patch needed it), so this
  task is now just the **wasm export** of `describe_device(type_id, configs)` + the **TS mirror** in
  `catalog.ts`. _Done:_ the descriptor for an 8×6-config computer carries 16 readouts / 48 grid params
  / 8-ch + 6-ch USB port faces over the wasm boundary; the EMPTY-config type catalog is unchanged;
  `cargo wasm` + full gate green.
- **Task 5.10.5 — Web: enumeration + adaptive faceplate.** `scene-ops` enumeration on USB duplex
  connect/disconnect (and interface removal): peer's published `channels` → computer config, matrix
  params reset to default, rebuild; `Computer.svelte`/`ComputerMixer.svelte` consume the
  per-instance descriptor (meters/grid resize; detected counts displayed read-only). _Done:_ Vitest
  for enumeration (connect 8i6 → 8×6; disconnect → 2×2; computer↔computer → no-op); in-browser —
  plugging the 8i6's single USB-C re-enumerates the computer (meters/grid resize) and the monitoring
  loop still sounds through the diagonal loopback, unplugging returns it to 2×2; `pnpm run format` +
  web `check`/`typecheck`/`test` green. **Note:** enumeration is a *gesture* (connect/disconnect), but
  the default scene ships the 8i6 pre-cabled to the computer — a loaded patch never enumerates, so the
  default scene **authors** the computer's 8×6 config (`scene-store.ts`, `SCHEMA_VERSION` 16→17). The
  per-instance descriptor is delivered to the UI by the worklet pushing a `deviceDescriptors` map (by
  scene id) on build and each hot-swap → `session.deviceDescriptors` → `session.descriptorOf(id)`, which
  the faceplate/focus render sites prefer over the static type catalog.

_Validate:_ an unattached computer presents **2×2** (its built-in audio); plugging the 8i6's single
USB-C cable **re-enumerates** it to 8 sends / 6 returns — the send meters and routing grid resize, and
the monitoring loop closes and sounds through the diagonal loopback; unplugging returns it to 2×2;
every channel count derives from the interface's **published** port face (no parallel constant
anywhere); the full Rust gate (`cargo fmt --check && cargo lint && cargo test && cargo wasm && cargo
docs`) plus web `check`/`typecheck`/`test` pass; verified in-browser.

### Story 5.11 — Computer as a minimal DAW — ✅ **Done** (Validate green, incl. overdub, in-browser)

_Progress:_ **All tasks 5.11.1–5.11.6 implemented and green** end to end. The Rust side (5.11.1–5.11.5):
the file-byte seam + WAV codec, the digital-domain transport, the `MultitrackRecorder` channel-strip
node, the `computer` as a 5-node channel-strip DAW, and the wasm DAW seam. The web side (5.11.6): the
OPFS storage worker (sync access handles), the worklet record/playback loop, the session orchestration +
track model (`SCHEMA_VERSION` 18), and the DAW **mixer** focus surface (transport + track strips +
routing + waveform with a scrolling playhead). Full gate green — **Rust** (engine 364 · devices 61 ·
wasm-bindings 13 · wasm · docs) and **web** (Biome · typecheck · Vitest 268). **In-browser: record → play
→ overdub all verified (audible)** — the Validate gate is met. The mixer topology
evolved twice mid-story (one-in-one-out → `(N+T)→M` crossbar → **channel-strip** `tracks → Matrix(T→M)`),
now final (see the headline note).

_Goal:_ Bring the `computer` forward from a fixed monitoring loopback into a **minimal, honest DAW** — the
digital hub a real audio interface plugs into. It grows an **arbitrary number of mono tracks**; each track
**arms** to a USB send (the interface's inputs), **records** its audio to a **file on disk**, and **plays
that file back** to a USB return (the interface's outputs), through an **in-simulation routing matrix** and
a **simple level mixer**. A single **transport** (play / stop / record) drives it, clocked by the
**in-simulation digital clock domain** — not the host's capture clock. Anchors to PROJECT_PLAN §4 (the
computer is a black-box device on real digital ports), §5.6 (clock is a real rate inside the sim, not a
label), and §2's "**Not a DAW**" non-goal — which this story respects by staying strictly at the
**signal-path** altitude (record → route → level → play), the routing/monitoring/gain-staging lesson, with
production features (timeline editing, clip slicing, automation, tempo/grid, per-track plugins) explicitly
out. Retires the `computer`'s diagonal-loopback `Matrix` default (Story 5.10) in favour of a multitrack recorder.

_Why it serves the learning goal (reconciling §2's non-goal):_ record-arm, send/return assignment, input
monitoring, the routing matrix, and gain-staging in and out of the interface are **core audio-engineering
routing knowledge** — impractical to explore with real gear. This is that lesson made hands-on, not a music
production tool. The scope ceiling below is the guardrail that keeps it honest to the non-goal.

_Watch out:_

- **Clock: the transport runs on the in-sim digital domain, never the host.** The one running counter today
  (`Schedule::sample_pos`, `schedule.rs:318`) is the **analog-rate** external-event clock; `SampleBuffer`
  carries **no** position (only `SampleRate`/`BitDepth`/`ClockDomainId`, always `ClockDomainId::SINGLE`).
  The DAW playhead is a **new digital-domain sample counter** advancing the digital block length (**128
  samples @ 48 kHz**, analog 1024 ÷ M = 8; `alloc_lane`, `schedule.rs:1181-1228`) per processed block when
  playing. Frame it as the digital domain's **own** counter (forward-compatible with Story 5.3, where the
  DAW clock can drift from the interface clock) — do **not** map it onto the host `AudioContext.currentTime`
  or the analog `sample_pos`. Epic 3 already concluded a transport must ride the engine's own sample clock +
  a shared reference, not host time (`EPIC_3_NOTES.md:391-398`).
- **Host = dumb byte storage; the sim owns all audio.** The **only** thing crossing the sim↔host boundary is
  **opaque file bytes** ("append these bytes to file X" / "read bytes of file X" → OPFS). The `computer`
  (in the sim) **WAV-encodes** recorded samples and **WAV-decodes** playback bytes itself, in the digital
  domain; format, rate, timeline, and mix never leak to the host. A real DAW writes WAVs to disk; OPFS is
  that disk. _One nuance, not a layer break:_ the UI may decode a stored WAV **host-side purely to draw a
  waveform thumbnail** — a filesystem read for display, not the host doing audio.
- **No unbounded engine buffer; the audio thread never blocks on disk.** Recording **streams per block**:
  `process` writes the block's samples to a **pre-allocated ring** (zero-alloc); off the hot path the sim
  WAV-encodes into an **outbound byte ring** the host drains to OPFS asynchronously. Playback is the mirror
  (host fills an **inbound byte ring** ahead of the playhead; the sim decodes per block into a playback
  ring; `process` reads it). Disk slowness surfaces as an honest under/overrun, not an audio glitch. The
  whole take **never** lives in engine memory.
- **Hot-path contracts hold.** `process` stays zero-alloc / panic-free / branch-light (sample rings only).
  WAV encode/decode and ring servicing run **off** `process` (like the readout-lane refresh / capture),
  with pre-sized scratch — no allocation on any path once compiled. Fixed known format ⇒ decode is a total
  fixed transform.
- **Determinism holds.** Playback bytes are **external input** on the same footing as note events — same
  fed stream ⇒ identical output. Recording is a pure tap (no effect on the live signal). No ambient entropy.
- **Don't break the default scene.** The mic/synth → 8i6 → computer → 8i6 → monitor loop must keep sounding:
  the default computer ships **one track input-monitoring send 1 → master (return 1)**, transport stopped,
  nothing recorded. This retires the all-diagonal loopback (a behaviour change — bump `SCHEMA_VERSION`). The
  **delayed USB-return output** (`delayed_outputs`, one block of round-trip latency) is unchanged.
- **Mono now, stereo later — foundation open, node mono-baked (honest status after 5.11.3).** The
  _primitives_ are all channel-agnostic and stereo-ready: `WavSpec { channels }` (interleaved WAV),
  `ByteRing` (byte-agnostic — interleaved L/R streams with frame = `channels × 4`, tears still impossible),
  `Transport` (channel-agnostic), and the **per-track** level fader (one fader = a whole stereo track). The
  `MultitrackRecorder` node itself, however, **bakes one-lane-per-track today** (`input` is one send lane per
  track; one output *channel* lane + one ring + one fader per track; a single-4-byte-frame per-sample loop;
  `set_input(track, lane)`) — and the `T→M` `Matrix` would likewise carry a stereo channel as two lanes —
  chosen for a simpler, fully-tested mono node, since the epic is mono-only and there is **no stereo source
  to exercise a stereo track end-to-end**. Going stereo is therefore a **contained node-local refactor**
  (per-track lane _list_ + `channels` count → interleaved-PCM rings → an inner per-channel `process` loop +
  `set_input(track, &[lane])`; the fader stays one-per-track), confined to `multitrack.rs` and its callers —
  **no change to `Transport`, `ByteRing`, `wav`, the schedule, or any other node.** Build it when a stereo use
  actually exists; not speculative infra now. _(Supersedes the earlier "a track owns a list of lanes (1
  today); no API that assumes one-lane-per-track" wording — the node does assume it; the foundation doesn't.)_
- **Scope guards:** signal-path + a simple level mixer (faders) only. **OUT:** timeline editing, clip
  slicing/arranging, automation, tempo/grid/quantize, per-track inserts/plugins, undo, multi-clip-per-track.
  One transport, tracks share it. No punch ranges — record = write from the playhead while transport rolls.

_Design notes (settled at planning):_

- **The mixing/routing/transport lives in the simulation, not the host (the headline decision).** The
  `computer` device owns the routing matrix (track → return), the per-track + master **level faders**, the
  input monitoring, and the digital-clock playhead. The host supplies only **raw file bytes** and issues
  transport **commands**; it does no audio. This aligns with the clocking call (the digital domain owns its
  timeline and mix bus) and the Epic-4/5 layer rule (engine = signal, web = app/UI + now the disk).
  _Rejected: a host-side mixer_ (engine exposes send lanes + injects pre-mixed return streams) — thinner
  engine, but the mixer/timeline wouldn't be "in the sim" and the transport clock would drift toward host
  ownership, against §5.6.
- **One bidirectional file-byte seam, not two audio taps.** Earlier framing had the engine stream raw audio
  samples to the host (record) and take samples back (playback) — which leaks audio format/rate/timeline to
  the host. Superseded: the seam is **opaque bytes for file storage** (SPSC-shaped byte rings each
  direction, bounded, under/overrun-on-pressure, serviced off the hot path), the host an OPFS filesystem.
  The sim owns the WAV codec. This also keeps the deferred **Story 4.7 waveform probe** independent — it's a
  different (display) mechanism, unbuilt here.
- **The transport playhead is a `u64` digital-domain counter, host-mirrored, engine-authoritative.** The
  engine advances it deterministically (128/block **while rolling**); the host predicts it (knows start
  position + roll state, same block advance) to pre-fill/drain byte rings without per-block feedback; on
  seek/stop the host resyncs; the engine's count is truth if they ever diverge (e.g. an overrun). This is
  the "carry a shared clock reference over the transport" conclusion (`EPIC_3_NOTES.md:391-398`), realized
  on the digital clock. Resembles the event `sample_pos`/`drain_due` shape (`schedule.rs:451-488`) — a due
  region `[pos, pos+block]` — but on the digital rate.
- **Overdub is the correctness bar: transport = rolling/stopped + an _independent_ record-enable (not a
  play-vs-record mode).** A real DAW records new tracks **while playing back already-recorded ones**. So
  playback and record are **independent per-track concerns processed every rolling block on the one
  playhead** — not mutually-exclusive global modes. While rolling: every track holding file data at the
  playhead **plays** (decode → route/sum → returns) **and**, in the very same block, every **armed +
  record-enabled** track **captures** its assigned send to its own file. This must **emerge** from the
  per-track structure (a track that plays and a track that records are just different per-track states in
  the same recorder loop), not be special-cased. _Rejected: a global record mode that supersedes playback_ — it
  would forbid overdubbing, the core multitrack act. Consequence for the seam (below): the byte streams are
  **per-track**, so N tracks can play distinct files while another writes its own. Known simplification: an
  overdubbed take lands at the **monitored** position — offset by the uncompensated round-trip latency (AD +
  USB + delayed return); record-latency compensation is out of scope (a later/5.3-era concern).
- **Multiple `computer`s in one scene must just work — keep the sim honest, don't special-case a single
  DAW.** A scene can hold more than one interface + computer, so nothing may assume "the" DAW. The
  two-computer case should **emerge** from per-device state, exactly like overdub emerges from per-track
  state: each `computer` resolves its **own** DAW node by device id (`BuiltScene::daw(device)` already does),
  its transport / tracks / faders / takes are per-device, and the host keys everything it stores or routes by
  **`(deviceId, track)`**, never a bare track index. _(Surfaced concretely at 5.11.6: OPFS take files are
  `take-<encoded deviceId>-<track>.wav`, so two computers' track 0 are independent files — a single-DAW
  filename would have silently overwritten one take with the other.)_ The test bar: adding a second computer
  needs no new code path — it records/plays independently because the design was per-device all along.
- **Clock provenance: the DAW's rate is the interface's, carried in the data — the transport hardcodes no
  rate (traced from code at planning).** What sets the digital rate is the **converter**, not the transport:
  the analog rate is the single `compile(graph, block_len, analog_rate, seed)` parameter (384 kHz); each
  `AdConverter`/`DaConverter` carries its own `SampleRate` (a distinct newtype from `AnalogRate`) and the AD
  **stamps** it onto the `SampleBuffer` it produces (`ad.rs` "opens a clock domain"); `compile`'s `alloc_lane`
  (`schedule.rs`) then derives `M = analog/digital`, sizes the digital lane to `block_len / M` (128 @ 48 kHz),
  and every digital→digital edge is compile-checked equal-rate (`ClockCrossingUnsupported` otherwise). So
  `Transport::advance(frames)` takes the frame count as an **argument** — the recorder passes the **runtime
  digital lane length** (`SampleBuffer::len()` of its USB lanes = `block_len / M` for whatever the interface
  feeds), never a `128` constant. This models **"the DAW follows the interface clock"** (the realistic USB
  case — the interface is the master, the computer slaves), so the "external clock → interface → DAW" story is
  the current mechanism, not a future one, and needs zero transport redesign. **What is _not_ emergent yet —
  all Story 5.3, already scoped there, none blocked by 5.11:** (1) a device declaring a **clock source**
  (`Internal`/`RecoverFrom`/`WordClock`) — `port.rs` flags `DigitalFace` as where the clock role will live;
  (2) `ClockDomainId::SINGLE` is hardcoded in `alloc_lane` (one domain ⇒ no drift, no async-boundary FIFO/
  slip); (3) cross-rate edges are **rejected, not resampled** (no SRC); (4) the DAW node rates are statically
  authored at 48 kHz in the catalog rather than **derived** from the interface's published port rate (the
  rate-axis analogue of 5.10's channel-count derivation — a no-op today since everything is 48 kHz). Building
  5.11 on the rate-agnostic `advance(frames)` keeps all four cleanly deferable.
- **Tracks are config-driven node sizing, exactly like 5.10's USB channels.** A hidden `track_count` config
  (written by the web track model) sizes the recorder; per-track routing/level/arm/monitor are runtime params
  (no recompile), the `Matrix` runtime-routing precedent (5.7.9). Adding/removing a track is a structural
  config change → recompile-on-change (the INST/48V/usb-channels pattern). _Rejected: a fixed max track
  count_ — encodes an arbitrary ceiling; config-driven is the established idiom.
- **Channel-strip mixer: track channels (fader + post-fader meter) → bus crossbar (the headline mixer
  decision, settled across 5.11.3).** The computer is a **5-node channel-strip console**, not a raw matrix:
  `DigitalMeter(N)` (send input meters, pre-fader) → **`MultitrackRecorder(N → T)`** (T **track channels**) →
  `DigitalMeter(T)` (**per-track after-fader meters**) → **`Matrix(T → M)`** (the **bus crossbar**) →
  `DigitalMeter(M)` (**return/bus meters**) → USB out (delayed). Each **track is a channel**: its signal is
  `(playback + monitored send) × per-track fader`, so an armed track hears its input *through its fader* like
  a real desk; the recorder outputs **one post-fader channel per track**, owns the [`Transport`], and records
  armed sends. Routing/summing lives in the `Matrix` crossbar (`tracks → returns`, default every track →
  return 0 = master; fan-out/aux = extra crosspoints). **Faders are per-track only** — you trim a live input
  at the **preamp**, not the DAW — and are driven over the wasm control seam (with transport/arm/monitor), not
  as exposed params, so no dynamic per-track param-labeling is needed. _Evolution:_ this superseded two
  earlier shapes in the same story — first one-in-one-out tracks (couldn't fan out), then a `(N+T)→M` crossbar
  with sends passed through (a raw matrix mixer with **no per-track fader**, so no after-fader metering). The
  channel-strip model is what per-track after-fader meters require: a distinguished fader per channel, tapped
  before the bus sum. **Consequence:** monitoring a live input now always goes *through a track* (arm/monitor
  a track whose source is that send) — exactly how a DAW works. Still expresses many-to-few (30 tracks → a
  2-lane master), fan-out/aux (a track's crossbar row hitting master *and* an aux return), and the mic/synth
  monitoring loop (the default 1 track monitors send 0 → master).
- **WAV codec is a small hand-rolled wasm-safe writer/reader in the engine** (a fixed canonical 44-byte
  header + PCM). Format is **32-bit IEEE float** (`WAVE_FORMAT_IEEE_FLOAT`, tag 3) — matches the DAW's own
  `f32` `SampleBuffer` storage, so encode→decode is **bit-exact** (no quantization step). Mono now, a
  `channels` field for stereo later. Decode is **total** (foreign bytes from host storage → `WavError`, never
  a panic; unknown chunks skipped). Stored files are real WAVs (nice-to-have: downloadable/inspectable).
  _Rejected: host-side WAV encoding_ — puts audio semantics in the host, against the seam decision.
  _Rejected: reusing `hound` (the community standard, already a `harness` dep) in the engine_ — it would
  compile to `wasm32` (pure Rust over `Read`/`Write`/`Seek`, supports float), but fits this use poorly: (a)
  its `WavWriter`+`finalize()`-`Seek` shape means holding the **whole take in a `Cursor<Vec<u8>>` in WASM
  memory** until finalize — exactly what the per-block streaming-to-disk model forbids; (b) it allocates and
  isn't shaped for the pre-sized, alloc-free, per-quantum framing this seam needs next to the audio thread;
  (c) the engine is deliberately dependency-lean and wasm-clean (the reason `hound` was quarantined to
  `harness`), and a dep to save ~90 lines of a *fixed-format* header + `f32::to_le_bytes` is a bad trade; (d)
  a crate's value is decoding **arbitrary** WAV variants, but we only ever decode files we wrote at one fixed
  format — variant-handling we'd never exercise. **Tipping point:** if we ever import arbitrary user WAVs
  (odd bit depths, ADPCM, extensible headers), switch to `hound` (or `symphonia` for broad decode) rather
  than grow a hand parser.
- **The "simple mixer" = per-track faders + a bus crossbar + three meter banks.** Level and routing are
  **distinct** (unlike a raw matrix mixer): a per-track **fader** (channel level, on the recorder) is metered
  **after** it (per-track post-fader `DigitalMeter(T)`), then the **`Matrix(T→M)`** crossbar routes/sums
  channels to buses (routing + aux-send levels), metered at the **buses** (`DigitalMeter(M)`). Plus the
  pre-fader **send input meters** for record levels. So three meter banks — **Send** (inputs), **Track**
  (after-fader), **Return** (bus) — rendered in the focus view (the 5.7.9 `RoutingGrid` + channel strips).
  Setting levels + routing only; no EQ/dynamics/pan. _Infra:_ `ReadoutSpec::PerNode` labels one bank per
  meter node (lane count derived from the node's own readout count, so the per-*track* bank sizes to `T`
  without a port total).
- **Storage transport: `postMessage` byte chunks to start, SAB later if needed.** The byte rings cross the
  worklet↔main boundary; begin with per-quantum `postMessage` of byte chunks (the current param/event
  transport shape), promote to a `SharedArrayBuffer` ring (the deferred Epic-3 SAB work) only if disk-rate
  bulk transfer demands it. Recorded per-block payload is small (128 frames). The `ByteRing` is hand-rolled
  and **single-threaded-shaped today** (like `EventQueue` is just a `Vec`), with a custom **all-or-nothing
  whole-frame** `write`/`read` guarantee (a slow consumer drops a whole block, a slow producer underruns a
  whole block — PCM never tears). _Rejected (for now): an audio-SPSC crate (`rtrb`/`ringbuf`)_ — `rtrb` gives
  element streaming **without** the whole-frame framing guarantee, and it's lock-free machinery not needed
  until the SAB upgrade. The `write`/`read` interface is shaped so `rtrb` can be evaluated to slot **beneath**
  it when that upgrade lands.
- **Known simplifications (not bugs):** mono tracks only; one clip per track (record replaces); no timeline
  editing/seeking-into-a-clip beyond transport seek; input monitoring is a per-track gate (not latency-
  compensated against the round-trip); the DAW clock is still the single domain today (drift vs. the
  interface is a Story 5.3 concern); OPFS-only (native harness may use real files behind the same seam).

- **Task 5.11.1 — Engine: the file-byte transport seam.** Bounded, SPSC-shaped **per-track byte streams**
  in each direction between the recorder and the host (outbound = each recording track's bytes to store;
  inbound = each playing track's file bytes) — **indexed by track** so N tracks play distinct files while
  another writes its own (the overdub requirement), serviced **off** the hot path (drop/underrun-on-pressure,
  no `process` involvement), host side opaque. Plus the wasm-safe **WAV codec** (writer/reader, minimal
  header + PCM, or raw f32). _Done:_ oracles — a known sample block WAV-encodes then decodes back
  **sample-exact** (hand-checked header + PCM bytes in a comment); two tracks' inbound streams decode
  independently without cross-talk while a third's outbound stream fills; ring under/overrun is bounded and
  lossy-not-panicky; no-alloc test green (rings pre-allocated; `process` untouched); `cargo wasm` green (no
  native-only dep).
- **Task 5.11.2 — Engine: transport + digital-domain playhead.** A `u64` digital-domain sample counter with
  a **rolling/stopped** transport + **seek**, and an **independent record-enable** (per-track arm decides who
  writes). Advances the digital block length per processed block **while rolling** (playback *and* any armed
  record happen on the same rolling playhead — no play-vs-record mode); recording writes are further gated by
  per-track arm + record-enable, playback reads by whether a track has file data. _Done:_ oracles — playhead
  advances **128/block @ 48 kHz** while rolling and holds while stopped (hand calc: analog 1024 ÷ M = 8);
  seek repositions exactly; record-enable toggles writes **without** stopping playback (the overdub gate);
  deterministic across runs.
- **Task 5.11.3 — Engine: the `MultitrackRecorder` node (track channels).** ✅ **Done.** A channel-strip
  node: `T` mono tracks, `N` send lanes in → **`T` post-fader track channels** out. Per track the channel is
  `(playback + monitored send) × per-track fader` written to its output lane; it **records** its assigned send
  (pre-fader) while rolling + record-enabled + armed. The **fader** is a recorder-owned framework `Smoother`
  driven over the control seam ([`set_track_level`], with `set_input`/`set_armed`/`set_monitoring`/transport),
  **not** an exposed param — so it de-zippers without dynamic param-labeling. Owns the [`Transport`], advancing
  it by the **runtime** lane length. **No routing/summing** (the downstream `Matrix` crossbar's job). _Delivered
  oracles (8):_ ports = N in / T out, **no params**; a default track monitors its send at unity; the monitor
  gate; the fader scales the channel once its 5 ms glide settles (`0.8·0.5 = 0.4`); playback streams through the
  channel only while rolling; record captures the pre-fader send only when rolling + record-enabled; **overdub
  oracle** — track 0's channel carries its file **unchanged** while track 1 records, same block; transport
  advances by the runtime lane length. Alloc-free (rings + smoothers pre-allocated; stack `[u8;4]`), panic-free
  (a direct `no_alloc.rs` check drives the rolling+recording+fader path). _(Superseded two intermediate shapes
  in-story — see the channel-strip headline note.)_
- **Task 5.11.4 — Devices: rebuild `computer` as the DAW.** ✅ **Done.** Rewrote the catalog entry to the
  **5-node channel-strip chain** `DigitalMeter(N)` → `MultitrackRecorder(N → T)` → `DigitalMeter(T)` →
  `Matrix::new_single_ports(T → M)` → `DigitalMeter(M)` → USB out (delayed), with `T` from a hidden
  `track_count` config (default **1**) and USB N/M from 5.10 (default 2×2). The crossbar default routes **every
  track → return 0 (master)** at unity — retiring the old diagonal send-k→return-k default. Generated grid
  labels `"Track i → Return j"` (rows = T track channels — derived from the matrix's crosspoint count, not the
  USB-In face — cols = M returns), plus **three meter banks** Send/Track/Return. _Infra:_ `describe` sizes grid
  rows from the matrix's own crosspoint count (`crosspoints / m_out`; `GridAxis::Named` self-sizes so the 8i6's
  14×14 is untouched), and `ReadoutSpec::PerNode` labels one bank per meter node (lane count from the node's
  readout count, so the per-track bank sizes to `T`); both alignment guards updated. _Delivered oracles:_
  default = 5 nodes / 4 edges, 2 crosspoints, **10 readouts** (2 send + 1 track + 2 return, ×2), still audible
  in the playable-loop test; `track_count`=4 → a 4×6 crossbar (24 crosspoints) + a 4-lane track meter; 8×6/4-tk
  descriptor labels `"Track 4 → Return 6"` + Send/Track/Return readouts; `ChannelCountMismatch` + duplex
  backstops intact; **the deferred 5.11.3 no-alloc proof landed here** (direct `no_alloc.rs` check of the
  recorder's rolling+recording+fader path). Full Rust gate green (engine 362 + devices 60). **Note for
  5.11.6:** the crossbar (now `T→M`) reshapes the matrix crosspoint ids and retires the diagonal default, so a
  saved scene's stored matrix `ParamSetting`s are stale → **bump `SCHEMA_VERSION`** (discard + rebuild) when the
  web lands.
- **Task 5.11.5 — wasm: export the DAW seams.** ✅ **Done.** On `SceneEngine`, exposed by **device id
  (+ track)**: transport commands (`transport_play`/`transport_stop`/`transport_record_enable`/
  `transport_seek`) + `playhead`/`is_rolling`/`is_recording` getters; the per-track control seam
  (`set_track_level`/`set_track_armed`/`set_track_monitoring`/`set_track_input`); and the byte transport
  (`feed_playback(device, track, &[u8]) -> bool` / `drain_record(device, track) -> Vec<u8>`).
  _As built:_
  - **The new hook, as designed.** A defaulted `Node`-trait hook `fn daw(&mut self) -> Option<&mut dyn
    DawControl> { None }` (the phantom-hook / `group_delay` precedent) beside a `DawControl` trait
    (`crates/engine/src/daw.rs`) carrying the ops — transport access, per-track controls, and the byte
    `feed_playback`/`drain_record` facade; `MultitrackRecorder` overrides both (forwards to its inherent
    methods / rings). `Schedule::node_mut(NodeId) -> Option<&mut (dyn Node + 'static)>` plus a
    `BuiltScene::daw(device)` resolver (probes each device's node ids at build for the one whose `daw()`
    is `Some`, stored in a `daw_nodes` map). `SceneEngine` routes each JS call `device → node → daw()`.
    _Gotcha:_ `node_mut`'s object needs the `'static` lifetime (the boxed nodes are `dyn Node + 'static`;
    an elided `+ '_` fails on `&mut` invariance).
  - **Positions cross as `f64`** (exact to 2^53 samples — millennia at 48 kHz), converted to the
    transport's `u64` internally — friendlier for JS than a BigInt `u64`. The getters take `&mut self`
    (reaching the node needs `node_mut`); off the hot path, so fine.
  - **Byte transport is copied chunks, NOT zero-copy** (correcting the earlier note): a `ByteRing` wraps
    around, so it has no contiguous region to view — and the postMessage-start model doesn't need zero-copy.
    JS passes/receives `&[u8]`/`Vec<u8>` (wasm-bindgen copy); per-block payload is tiny (~512 B/track). The
    SAB zero-copy ring stays the deferred optimization.
  - **The worklet loop** (5.11.6 territory, noted here): each quantum, after `render_quantum`, drain every
    track's record ring → post to main → OPFS append; and feed every track's playback ring from bytes main
    pre-fed (OPFS read ahead of the playhead).
  _Done:_ the byte rings + transport + track controls round-trip across the wasm boundary for an N-track
  computer; the seam is total on non-DAW devices (silent no-ops); the EMPTY-config type catalog unchanged;
  full Rust gate green (engine 364 · devices 61 · wasm-bindings 11 · `cargo wasm`/`docs`).
- **Task 5.11.6 — Web: OPFS storage + track model + transport UI + level mixer + waveform.** ✅ **Done**
  (in-browser record→play verified; overdub is the remaining live check). OPFS-backed take files (worker +
  sync access handles) draining/filling the byte rings around the playhead; a host-side **track model**
  (create/remove tracks → `track_count` config + recompile; arm; input/output assign; level); **transport
  controls** (record/play/stop); the **level mixer** (faders over the DAW control seam — **not** params;
  see below) + routing in the focus view (the 5.7.9 `RoutingGrid` precedent); a **waveform** view of stored
  takes with a scrolling playhead cursor, host-side, display only.
  _As built (the JS-side architecture — three contexts + wasm):_
  - **AudioWorklet** = the only wasm instance: drains recorded PCM, brackets each take with
    `recordStarted`/`recorded`/`recordStopped` (building the WAV headers via the sim's `wav_header` — it
    owns the codec + the take lifecycle), and posts throttled transport state. **Storage Worker** = OPFS
    per-file **sync access handles** (`take-<encoded deviceId>-<track>.wav`), pure byte I/O, no wasm.
    **Main/session** = orchestrator: relays record bytes worklet→worker, and feeds each playing ring by
    **playhead occupancy** (`fed − (playhead−playStart)×4 < highwater`, the pure `planPlaybackFeed`).
  - **Faders are driven over the DAW control seam (`set_track_level`), not params** — correcting this
    task's original "faders = recorder params" sketch; the recorder-owned smoother (decided at 5.11.3/5.11.5)
    is the as-built design, so the mixer's fader calls the seam, not a `ParamHandle`.
  - **Per-track state persists in `SceneUi.tracks`** (engine track state is runtime-only, reset on every
    recompile) and is re-applied after each build via `applyTrackState`. `SCHEMA_VERSION` **17 → 18** (the
    crossbar reshaped the matrix crosspoint ids + the new track model) — stale saves discard.
  - **Multi-computer stays honest** (per the design note): takes are keyed by `(deviceId, track)`, so a
    second computer needs no new code path.
  Vitest covers the host logic — `take-store` (streaming record + the overdub read-while-write), `storage-client`
  (client↔protocol↔store over a fake worker), `daw` (`planPlaybackFeed`), and `waveform` (reduction + cursor
  length). _Done:_ in-browser — arm a track to the mic/synth send, hit record, stop, play it back through the
  monitoring loop and **hear it** (✅ verified); a track sums into master; `pnpm run format` + web
  `check`/`typecheck`/`test` green (✅). **Remaining live check:** overdub — with a take playing, arm a
  second track, record it, and hear both together on the next playback (the DAW pressure test).

_Validate:_ the `computer` presents an **arbitrary number of mono tracks**; a track **armed** to a USB send
**records to a WAV file on disk (OPFS)** while the transport rolls, and **plays that file back** to a USB
return through the **in-sim routing matrix + level mixer**, audible through the interface monitoring loop;
the **transport** (rolling/stopped + record-enable) is clocked by the **in-simulation digital domain**
(playhead advances 128 digital samples/block, never touching the host clock); **overdub works — a new track
records while already-recorded tracks play back on the same playhead**, and both are heard together
afterward (the DAW pressure test); the **only** sim↔host data is **opaque file bytes** (host stores; the sim
owns WAV encode/decode, routing, levels, timeline); the recorded take **never lives whole in engine memory**
and the audio thread never blocks on disk; the default scene's mic/synth loop
still sounds (one default monitor track); mono-now with a lane-list door open for stereo; hot path stays
zero-alloc / panic-free and deterministic; the full Rust gate (`cargo fmt --check && cargo lint && cargo
test && cargo wasm && cargo docs`) plus web `check`/`typecheck`/`test` pass; verified in-browser.
