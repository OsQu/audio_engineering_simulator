//! Proves the hot path is allocation-free: [`Schedule::process`] must not allocate.
//!
//! A panic-or-glitch in a WASM AudioWorklet kills the audio stream, and allocation is one of
//! the surest ways to glitch — so the zero-alloc contract is verified, not just asserted in
//! prose. We install a global allocator that counts every allocation, then check that running
//! many blocks through a compiled schedule adds **zero** to that count.
//!
//! This is a separate integration-test crate (its own binary) so its `#[global_allocator]`
//! is isolated from the library's unit tests.
#![allow(
    unsafe_code,
    reason = "implementing GlobalAlloc (an unsafe trait) is the only way to observe allocations; \
              the impl merely counts and forwards to the System allocator"
)]

use engine::{
    AnalogRate, BalancedDriver, BalancedReceiver, BitDepth, Cable, ClockDomainId, DcBlocker,
    EventMessage, EventQueue, EventThru, Farads, GainStage, Graph, InputZ, Lane, MultitrackRecorder,
    Node, NoiseDensity, Ohms, ParamQueue, Params, PassiveSum, SampleBuffer, SampleRate, SynthVoice,
    TestSource, VoltageBuffer, Volts, compile,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Counts allocations, otherwise delegating to the system allocator.
struct CountingAlloc;

static ALLOCS: AtomicUsize = AtomicUsize::new(0);

// SAFETY: every method forwards to `System` with the exact same arguments it received, so the
// allocator contract is upheld; we only add an atomic increment alongside `alloc`.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: forwarding the caller's layout to the system allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarding the caller's ptr+layout back to the system allocator.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

fn rate() -> AnalogRate {
    AnalogRate::new(384_000.0)
}

// NB: these checks share one process-global `ALLOCS` counter, so they must run **serially** — the
// single `#[test]` below calls them in sequence. Splitting them into separate `#[test]`s would let
// the harness run them in parallel, and one test's setup allocations would corrupt the other's count.
#[test]
fn process_paths_are_allocation_free() {
    analog_chain_is_allocation_free();
    voice_with_events_and_params_is_allocation_free();
    multitrack_recorder_is_allocation_free();
}

fn analog_chain_is_allocation_free() {
    // source → (cable) → gain → dc-blocker → sum → balanced driver → balanced receiver. The
    // cabled edge exercises the one-pole low-pass, the gain stage carries a noise floor, the DC
    // blocker is the one-pole high-pass, and the driver→receiver hop is a two-conductor balanced
    // edge — so the cable filter loop, the per-sample Gaussian draw, the high-pass step, and the
    // per-conductor balanced edge transforms are all covered by the no-alloc check.
    let mut g = Graph::new();
    let src = g.add(TestSource::new(Volts::new(1.0), Ohms::new(100.0)));
    let amp = g.add(
        GainStage::new(
            2.0,
            Volts::new(10.0),
            InputZ::new(Ohms::new(10_000.0)),
            Ohms::new(150.0),
        )
        .with_noise(NoiseDensity::new(10e-9)),
    );
    let dc = g.add(DcBlocker::new(
        Farads::new(31.831e-9),
        Ohms::new(1_000_000.0),
        Ohms::new(150.0),
    ));
    let sum = g.add(PassiveSum::new(
        vec![InputZ::new(Ohms::new(10_000.0))],
        Ohms::new(150.0),
    ));
    let drv = g.add(BalancedDriver::new(
        InputZ::new(Ohms::new(1e9)),
        Ohms::new(1.0),
    ));
    // A per-conductor DC blocker lifted onto the balanced pair: its two replicated lanes run on
    // the hot path, so the lift's per-leg processing is covered by the no-alloc check too.
    let bal_dc = g.add(DcBlocker::new(
        Farads::new(15.915e-9),
        Ohms::new(10_000.0),
        Ohms::new(150.0),
    ));
    let rcv = g.add(BalancedReceiver::new(Ohms::new(1e9), Ohms::new(150.0)));
    g.connect_cabled(
        src,
        0,
        amp,
        0,
        // Pickup + hum on the cable exercise the edge's per-sample interference (Gaussian draw and
        // 50/60 Hz generator) in the alloc check.
        Cable::new(Ohms::new(100.0), Farads::new(1e-9))
            .with_pickup(NoiseDensity::new(10e-9))
            .with_hum(60.0, Volts::new(0.01)),
    );
    g.connect_ideal(amp, 0, dc, 0);
    g.connect_ideal(dc, 0, sum, 0);
    g.connect_ideal(sum, 0, drv, 0);
    g.connect_ideal(drv, 0, bal_dc, 0);
    g.connect_ideal(bal_dc, 0, rcv, 0);
    g.set_output(rcv, 0);

    let mut sched = compile(g, 64, rate(), 0).expect("valid chain");
    let mut out = VoltageBuffer::zeros(64, rate());

    // Everything is allocated by `compile`; `process` must touch the allocator zero times —
    // including the very first call (no lazy first-block allocation).
    let before = ALLOCS.load(Ordering::Relaxed);
    for _ in 0..128 {
        sched.process(&mut out);
    }
    let after = ALLOCS.load(Ordering::Relaxed);

    assert_eq!(
        before,
        after,
        "process() allocated {} time(s) over 128 blocks",
        after - before
    );
}

fn multitrack_recorder_is_allocation_free() {
    // The DAW record/playback hot path: while rolling *and* recording, `process` streams a playback
    // PCM frame out of each track's inbound ring and a record frame into its outbound ring per sample,
    // passes the sends through, and advances the transport — all of which must stay off the allocator.
    // Driven directly (not via a schedule) since the transport/rings are node-owned; the rings are fed
    // in setup, then under/overrun (silence / dropped frames) inside the measured loop — never allocate.
    let rate = SampleRate::new(48_000.0);
    let bits = BitDepth::new(24);
    let (n_sends, n_tracks) = (2, 1);
    let mut rec = MultitrackRecorder::new(rate, bits, n_sends, n_tracks);
    rec.playback_ring_mut(0)
        .expect("track 0")
        .write(&vec![0u8; 4096]); // some playback to stream; underruns to silence later (no alloc)
    rec.set_armed(0, true);
    rec.set_track_level(0, 0.5); // a mid-glide fader so `value_at`/`advance` run in the measured loop
    rec.transport_mut().play();
    rec.transport_mut().set_record_enabled(true);

    let block = 128;
    let inputs: Vec<Lane> = (0..n_sends)
        .map(|_| {
            Lane::Sample(SampleBuffer::from_samples(
                vec![0.5; block],
                rate,
                bits,
                ClockDomainId::SINGLE,
            ))
        })
        .collect();
    let mut outputs: Vec<Lane> = (0..n_tracks)
        .map(|_| Lane::Sample(SampleBuffer::zeros(block, rate, bits, ClockDomainId::SINGLE)))
        .collect();

    let before = ALLOCS.load(Ordering::Relaxed);
    for _ in 0..128 {
        rec.process(&Params::EMPTY, &inputs, &mut outputs);
    }
    let after = ALLOCS.load(Ordering::Relaxed);

    assert_eq!(
        before,
        after,
        "MultitrackRecorder::process allocated {} time(s) over 128 blocks",
        after - before
    );
}

fn voice_with_events_and_params_is_allocation_free() {
    // The full input path: a synth voice driven by the event lane (note on/off) and a smoothed
    // control param (level), through `process_io`. Covers what `process()` alone can't — event
    // delivery into a lane, the voice consuming the Events lane, and the param de-zipper advance —
    // all of which must stay off the allocator on the hot path. The events reach the voice through
    // an `EventThru` controller (host-fed open input → routed edge → voice), so the pass-through's
    // per-block `copy_from` is covered by the no-alloc check too.
    let block = 64;
    let mut g = Graph::new();
    let ctrl = g.add(EventThru::new(64));
    let voice = g.add(SynthVoice::new(Volts::new(1.0), Ohms::new(150.0)));
    g.connect_ideal(ctrl, 0, voice, 0);
    g.set_output(voice, 0);
    let mut sched = compile(g, block, rate(), 0).expect("valid voice chain");
    let ev = sched
        .event_input(ctrl, 0)
        .expect("the controller's open event input");
    let level = sched.param(voice, SynthVoice::LEVEL).expect("level param");

    // Queue events spread across several blocks and a couple of param moves — all pre-allocated,
    // so the pushes themselves don't allocate, and delivery/application happen inside the measured
    // loop below.
    let mut events = EventQueue::with_capacity(16);
    events.push(
        0,
        ev,
        EventMessage::NoteOn {
            note: 69,
            velocity: 100,
        },
    );
    events.push((block as u64) * 4, ev, EventMessage::NoteOff { note: 69 });
    events.push(
        (block as u64) * 8,
        ev,
        EventMessage::NoteOn {
            note: 72,
            velocity: 80,
        },
    );
    let mut params = ParamQueue::with_capacity(4);
    params.set(level, 2.0);

    let mut out = VoltageBuffer::zeros(block, rate());

    let before = ALLOCS.load(Ordering::Relaxed);
    for _ in 0..128 {
        sched.process_io(&mut out, &mut params, &mut events);
    }
    let after = ALLOCS.load(Ordering::Relaxed);

    assert_eq!(
        before,
        after,
        "process_io() allocated {} time(s) over 128 blocks",
        after - before
    );
}
