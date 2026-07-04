# Plan — round-trip latency (delayed edges) to close the 8i6 ↔ computer loop

**Status:** ✅ IMPLEMENTED (Task 5.7.8). Design approved (delayed-edge model + option A: latency baked into
the `computer`). All five work-breakdown steps landed green — engine `connect_delayed` primitive (topo cut +
pre-loop copy) with oracles, multichannel/digital delayed edges, the `computer` declaring its USB output
delayed (`CatalogEntry.delayed_outputs`) + `build_patch` wiring it, and the true-loop default scene + docs.
The full loop `synth → 8i6 → computer → 8i6 → speaker` builds and is audible. This doc is kept as the design
record; retire it with `story_5_7_part2_plan.md` at the end of Story 5.7.
**Why now:** Task 5.7.8's validate wants the classic loop *audible in-browser through the computer*
(mic/synth → preamp → AD → USB → computer → USB return → DA → monitor). That loop is currently
**unbuildable**: it is a graph cycle (`build_patch` → `Err(Compile(Cycle))`), because the 8i6's single
14×14 `Matrix` node sits on **both** the USB-send and USB-return paths, and the computer closes the
loop `matrix → USB send → computer → USB return → matrix`. Confirmed empirically (throwaway probe).

## The tension with §5

`PROJECT_PLAN` §5 / `CLAUDE.md` §5 are explicit: *"Local solve only… the signal graph is a **DAG** —
a cycle is a wiring mistake the compiler rejects rather than a feedback path to resolve."* A real
interface↔DAW round-trip is **not** an instantaneous feedback path — it carries the DAW's buffer
**latency** (tens of ms). So the honest model is not "resolve a feedback loop" but "express a
**one-block-latency** edge." The framing that preserves the non-negotiable:

> **The *schedule* stays a strict DAG.** A round-trip is a **delayed edge**: cut from the topo sort
> (so it never forms a scheduling cycle) and served from the **persistent output pool** (last block's
> value). No same-block feedback solve is ever performed — the invariant holds; we only add *bounded
> latency*, which is physically what a DAW round-trip is.

This is the same spirit as the deferred clock-domain / ground-loop work: an emergent physical property
(here, latency) modeled cheaply, not a flag.

## The mechanism (grounded in the current engine)

Recon facts (verified in `schedule.rs`):
- `output_pool` / `input_pool` are **persistent** `Schedule` fields; **nothing zeroes them per block**
  (only event lanes are cleared). A node overwrites its output range when it runs. So at block start
  the pool still holds **last block's** outputs — a ready-made one-block register.
- `process_io` runs `steps` in topo order: `Step::Node` (read input range → write output range) and
  `Step::Edge`/`Connection` (copy `output_pool[src] → input_pool[dst]`).
- The topo deps are built node-level: `deps = edges.map(|e| (e.from_node, e.to_node))`
  (`schedule.rs:675`).

**Design:** a **delayed edge** is a normal connection with two changes:
1. **Excluded from the topo deps** — so a loop containing it is *not* a cycle (the schedule stays a
   DAG over the remaining edges).
2. Its copy step runs **before the main step loop**, reading the producer's **persistent** output-pool
   buffer — which at that moment still holds **last block's** value (the producer hasn't run yet this
   block). That delivers exactly **one block of latency**; block 0 reads the initial silence.

No new stateful node, no post-pass, no extra allocation on the hot path — just (a) a per-edge
`delayed: bool`, (b) filtering those out of `deps`, and (c) emitting their copies in a pre-loop pass.
Determinism holds (initial pool is deterministic silence).

### Correctness sketch (one delayed edge producer P → consumer C)
- Block N, **pre-loop**: `C.input ← P.output_pool` = P's block **N−1** output (P hasn't run yet).
- Block N, main loop (topo, delayed edge cut): C consumes block-N−1 data; … P runs, overwrites
  `P.output_pool` with block-N output.
- Block N+1 pre-loop: `C.input ← P.output_pool` = P's block-N output. ✓ exactly one block late.

## Decision 1 — where the latency lives (needs your call)

- **(A, recommended) Bake it into the `computer`.** The DAW's playback is inherently one buffer behind
  its input, so *the computer's USB output is a latency source*. A device can declare an output as
  "delayed"; `build_patch` marks edges **from** that device's delayed output as delayed. Consequence:
  connecting a computer *just works* — the round-trip loop builds with no user action, and the latency
  is physically attributed to the DAW (correct). Nothing else in the catalog is delayed.
- (B) A standalone `BlockDelay` device the user patches into a loop. Explicit and general, but clutters
  the patch and pushes "you must insert a delay" onto the user.
- (C) A per-connection "delayed cable" toggle. Flexible, but makes latency a wiring choice rather than
  a device property, and the UI must expose it.

A is the most faithful and lowest-friction. B/C are more general but I don't think we need generality
yet — only the computer needs latency today.

## Decision 2 — how "delayed output" is declared

If A: add a small seam so a **catalog entry** can mark an exposed output as latency-bearing (e.g. the
computer's USB Out). Options: a node-trait hint (`fn output_latency(&self, port) -> usize`, default 0)
read during `instantiate`, **or** a `CatalogEntry`/`PortUi` flag consumed by `build_patch`. Leaning
toward the **descriptor/build side** (it's a product property of the computer, not engine physics) —
`build_patch` marks inter-device edges whose source is a declared-delayed device output. To keep it
clean, the *engine* just gains `Graph::connect_delayed` (or a `delayed` field on the edge); the
*devices* layer decides which edges use it.

## Work breakdown (task-level, once the design is approved)

1. **Engine — delayed edge primitive.** Add `delayed: bool` to the graph edge + `Graph::connect_delayed`
   (or a param on connect). `compile`: exclude delayed edges from `deps` (cycle-break); emit their
   copy steps in a **pre-loop pass** in `process_io`. Keep everything alloc/panic-free.
   - _Oracles:_ a two-node loop `A ⇄ B` with one delayed edge **compiles** (no `Cycle`); a unit-step
     into a delayed edge appears at the consumer **one block later**; a non-delayed cycle still errors.
2. **Engine — multichannel + digital delayed edges.** The 8i6↔computer edges are 8- and 6-lane digital;
   ensure the delayed pre-loop copy handles the same lane/domain cases as the normal edge branch.
3. **Devices — declare the computer's USB output as delayed** and have `build_patch` mark those edges.
   - _Oracle:_ the full loop patch (synth → 8i6 → computer → 8i6 → speaker) **builds** and is
     **audible** (speaker peak > 0) — the test that currently would be `Err(Cycle)`.
4. **Web — the true-loop default scene.** Replace the placeholder default scene with the playable
   round-trip: controller → synth → 8i6 (combo in, matrix identity: Pre1→USB1) → computer (loopback
   send1→ret1) → 8i6 (USB return → matrix DAW1→Line1 → monitor DA/amp) → speaker; Power/Monitor set so
   it sounds on load. `SCHEMA_VERSION` bump if the scene shape changes materially.
5. **Docs.** Note the latency model in `PROJECT_PLAN`/`IMPLEMENTATION_PLAN` (the DAG invariant is
   preserved; round-trips are bounded-latency delayed edges) and in `osku_*` concept refs if it teaches
   a new idea. Update 5.7.8's validate to reflect the loop is now genuinely through the computer.

## Validate

The round-trip loop builds and is audible in-browser through the computer; a delayed-edge two-node loop
compiles while an undelayed one still errors; the unit-delay oracle shows exactly one block of latency;
full Rust gate + web suite green.
