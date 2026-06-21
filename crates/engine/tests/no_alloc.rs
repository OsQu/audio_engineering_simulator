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
    AnalogRate, BalancedDriver, BalancedReceiver, Cable, DcBlocker, Farads, GainStage, Graph,
    InputZ, NoiseDensity, Ohms, PassiveSum, TestSource, VoltageBuffer, Volts, compile,
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

#[test]
fn process_is_allocation_free() {
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
        Cable::new(Ohms::new(100.0), Farads::new(1e-9)),
    );
    g.connect(amp, 0, dc, 0);
    g.connect(dc, 0, sum, 0);
    g.connect(sum, 0, drv, 0);
    g.connect(drv, 0, bal_dc, 0);
    g.connect(bal_dc, 0, rcv, 0);
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
