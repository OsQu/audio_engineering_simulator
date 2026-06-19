//! Render/CLI test harness for driving the engine offline.
//!
//! This is the visualization *demo* (a detour after Story 1.3): it drives a sine through a
//! real compiled schedule and plots the resulting voltage in the terminal, so amplitude,
//! rail clipping, and cable rolloff can be *seen*, not just asserted in unit tests. The WAV
//! render driver and offline scenarios proper arrive in Epic 2.

mod sine;

use engine::{AnalogRate, Node, Ohms, VoltageBuffer, Volts};
use sine::SineSource;

fn main() {
    // Smoke check: a 1 kHz sine through one block, off a real Thévenin output. The plotted
    // scenarios (clean gain, clipping, cable rolloff) replace this in the next task.
    let mut src = SineSource::new(Volts::new(1.0), 1_000.0, Ohms::new(100.0));
    let mut out = VoltageBuffer::zeros(384, AnalogRate::new(384_000.0));
    src.process(&[], std::slice::from_mut(&mut out));
    let peak = out.as_slice().iter().fold(0.0_f32, |m, &v| m.max(v.abs()));
    println!("SineSource smoke: peak {peak:.3} V over one 1 kHz cycle");
}
