//! Proves the worklet's audio-thread calls neither allocate nor free: the [`SceneEngine::render_quantum`]
//! that **installs a hot-swapped scene**, and the per-quantum record drain.
//!
//! In an AudioWorklet, `process()` runs on the audio thread, and freeing a whole scene there (its
//! schedule's buffers, nodes, FIR kernels) is as much a glitch risk as allocating. The installing
//! quantum must only *move* the old scene aside; it is freed later, from a message-handler call.
//!
//! A separate integration-test binary so its counting `#[global_allocator]` is isolated from the
//! library's unit tests.
#![allow(
    unsafe_code,
    reason = "implementing GlobalAlloc (an unsafe trait) is the only way to observe allocations; \
              the impl merely counts and forwards to the System allocator"
)]

use devices::{ConfigSetting, Connection, DeviceInstance, ParamSetting, Patch, PortRef};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use wasm_bindings::SceneEngine;

/// Counts allocations and deallocations, otherwise delegating to the system allocator.
struct CountingAlloc;

static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static FREES: AtomicUsize = AtomicUsize::new(0);

// SAFETY: every method forwards to `System` with the exact same arguments it received, so the
// allocator contract is upheld; we only add an atomic increment alongside.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: forwarding the caller's layout to the system allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        FREES.fetch_add(1, Ordering::Relaxed);
        // SAFETY: forwarding the caller's ptr+layout back to the system allocator.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        FREES.fetch_add(1, Ordering::Relaxed);
        // SAFETY: forwarding the caller's arguments to the system allocator.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

/// `(allocations, deallocations)` performed while running `f`.
fn heap_traffic<R>(f: impl FnOnce() -> R) -> ((usize, usize), R) {
    let (a0, f0) = (
        ALLOCS.load(Ordering::Relaxed),
        FREES.load(Ordering::Relaxed),
    );
    let r = f();
    let (a1, f1) = (
        ALLOCS.load(Ordering::Relaxed),
        FREES.load(Ordering::Relaxed),
    );
    ((a1 - a0, f1 - f0), r)
}

fn dev(id: &str, type_id: &str) -> DeviceInstance {
    DeviceInstance {
        id: id.into(),
        type_id: type_id.into(),
        params: vec![],
        config: vec![],
    }
}

fn conn(from: &str, from_port: u32, to: &str, to_port: u32) -> Connection {
    Connection {
        from: PortRef {
            device: from.into(),
            port: from_port,
        },
        to: PortRef {
            device: to.into(),
            port: to_port,
        },
        cable: None,
        duplex: false,
    }
}

/// `synth → 8i6 ⇄ computer (8×6 DAW) → 8i6 Line Out → speaker`, the 8i6 powered on by a saved param
/// (so the installing quantum also applies a non-empty initial-param list). A real FIR-heavy scene.
fn daw_patch() -> Patch {
    let mut interface = dev("if", "scarlett_8i6");
    interface.params = vec![ParamSetting {
        id: 205, // the 8i6's device-level power switch
        value: 1.0,
    }];
    let mut computer = dev("computer", "computer");
    computer.config = vec![
        ConfigSetting {
            key: "usb_sends".into(),
            value: 8.0,
        },
        ConfigSetting {
            key: "usb_returns".into(),
            value: 6.0,
        },
    ];
    Patch {
        devices: vec![
            dev("synth", "synth_voice"),
            interface,
            computer,
            dev("spk", "speaker"),
        ],
        connections: vec![
            conn("synth", 0, "if", 0),
            conn("if", 0, "computer", 0),
            conn("computer", 0, "if", 7),
            conn("if", 2, "spk", 0),
        ],
        output: PortRef {
            device: "spk".into(),
            port: 0,
        },
    }
}

// NB: one `#[test]` — the counters are process-global, so parallel tests would corrupt each other.
#[test]
fn audio_thread_calls_neither_allocate_nor_free() {
    let mut engine = SceneEngine::from_patch(&daw_patch()).expect("the DAW patch builds");
    for _ in 0..4 {
        engine.render_quantum();
    }

    // --- Hot-swap install: the quantum that installs the queued scene moves the old one aside. ---
    engine
        .queue_patch(&daw_patch())
        .expect("the replacement builds");
    let (traffic, ()) = heap_traffic(|| engine.render_quantum());
    assert_eq!(
        traffic,
        (0, 0),
        "the installing render_quantum neither allocates nor frees (allocs, frees)"
    );
    assert!(
        engine.has_retired_scene(),
        "the old scene is parked, not freed"
    );

    // The next message-handler call frees it (proves the counter sees the scene's frees at all).
    let (traffic, ()) = heap_traffic(|| engine.transport_play("computer"));
    assert!(traffic.1 > 0, "the control call frees the retired scene");
    assert!(!engine.has_retired_scene());

    // --- Record drain: arm a track, record a few quanta, drain into the engine's scratch. -------
    engine.transport_record_enable("computer", true);
    engine.set_track_armed("computer", 0, true);
    for _ in 0..4 {
        engine.render_quantum();
    }
    let (traffic, n) = heap_traffic(|| engine.drain_record("computer", 0));
    assert!(n > 0, "the armed track recorded bytes to drain");
    assert_eq!(n % 4, 0, "whole f32 frames");
    assert_eq!(traffic, (0, 0), "draining the record stream is heap-silent");

    // Steady rendering + an empty drain stay heap-silent too.
    let (traffic, n) = heap_traffic(|| {
        let mut drained = engine.drain_record("computer", 0);
        for _ in 0..8 {
            engine.render_quantum();
        }
        drained += engine.drain_record("computer", 0);
        drained
    });
    assert!(n > 0);
    assert_eq!(traffic, (0, 0), "steady quanta + drains are heap-silent");
}
