//! Core voltage engine.
//!
//! The analog domain is modeled as a real, oversampled voltage waveform in volts —
//! physical behavior (levels, impedance loss, clipping, noise, DC, hum) emerges from
//! the voltage math rather than being flagged. See `PROJECT_PLAN.md` and
//! `IMPLEMENTATION_PLAN.md` for the design; this crate stays portable to `wasm32`
//! (no `std::thread`, no ambient `std::time`).

mod electrical;
mod fir;
mod graph;
mod level;
mod node;
mod noise;
mod param;
mod port;
mod rng;
mod schedule;
mod signal;
#[cfg(test)]
mod test_util;

pub use electrical::{Cable, Farads, InputZ, Ohms, OnePole, OutputZ, Thevenin, divider_gain};
pub use fir::{Decimator, Interpolator, kaiser_beta};
pub use graph::{Graph, NodeId};
pub use level::{
    dbu_to_volts, dbv_to_volts, headroom_db, sample_to_dbfs, volts_to_dbu, volts_to_dbv,
};
pub use node::{
    AdConverter, BalancedDriver, BalancedReceiver, CondenserMic, DaConverter, DcBlocker, GainStage,
    Node, PassiveSum, TestSource,
};
pub use noise::NoiseDensity;
pub use param::{ParamDecl, ParamHandle, ParamId, ParamQueue, Params};
pub use port::{AudioFormat, DigitalFace, EventFace, InputPort, OutputPort};
pub use rng::Rng;
pub use schedule::{CompileError, EventInputId, EventQueue, Schedule, ScheduleSlot, compile};
pub use signal::{
    AnalogRate, BitDepth, ClockDomainId, Domain, EventBuffer, EventMessage, Lane, SampleBuffer,
    SampleRate, TimedEvent, VoltageBuffer, Volts,
};
