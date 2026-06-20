//! Core voltage engine.
//!
//! The analog domain is modeled as a real, oversampled voltage waveform in volts —
//! physical behavior (levels, impedance loss, clipping, noise, DC, hum) emerges from
//! the voltage math rather than being flagged. See `PROJECT_PLAN.md` and
//! `IMPLEMENTATION_PLAN.md` for the design; this crate stays portable to `wasm32`
//! (no `std::thread`, no ambient `std::time`).

mod electrical;
mod graph;
mod level;
mod node;
mod noise;
mod rng;
mod schedule;
mod signal;
#[cfg(test)]
mod test_util;

pub use electrical::{Cable, Farads, InputZ, Ohms, OnePole, OutputZ, Thevenin, divider_gain};
pub use graph::{Graph, NodeId};
pub use level::{dbu_to_volts, dbv_to_volts, volts_to_dbu, volts_to_dbv};
pub use node::{GainStage, Node, PassiveSum, TestSource};
pub use noise::NoiseDensity;
pub use rng::Rng;
pub use schedule::{CompileError, Schedule, ScheduleSlot, compile};
pub use signal::{AnalogRate, VoltageBuffer, Volts};
