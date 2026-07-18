//! Core voltage engine.
//!
//! The analog domain is modeled as a real, oversampled voltage waveform in volts —
//! physical behavior (levels, impedance loss, clipping, noise, DC, hum) emerges from
//! the voltage math rather than being flagged. See `PROJECT_PLAN.md` for the design;
//! this crate stays portable to `wasm32` (no `std::thread`, no ambient `std::time`).

mod byte_ring;
mod dsp;
mod electrical;
mod fir;
mod graph;
mod level;
mod node;
mod noise;
mod param;
mod port;
mod readout;
mod rng;
mod schedule;
mod signal;
#[cfg(test)]
mod test_util;
mod transport;
mod wav;

pub use byte_ring::ByteRing;
pub use dsp::{Biquad, PeakEnvelope};
pub use electrical::{
    Cable, Farads, InputZ, Ohms, OnePole, OutputZ, PhantomLoad, PhantomSupply, Thevenin,
    divider_gain,
};
pub use fir::{Decimator, Interpolator, kaiser_beta};
pub use graph::{Graph, NodeId};
pub use level::{
    dbu_to_volts, dbv_to_volts, headroom_db, sample_to_dbfs, volts_to_dbu, volts_to_dbv,
};
pub use node::{
    AdConverter, BalancedDriver, BalancedReceiver, Compressor, CondenserMic, DaConverter,
    DcBlocker, DigitalDemux, DigitalMeter, DigitalMux, EqBand, EventThru, GainStage, Matrix,
    MicPreamp, Node, PassiveSum, Speaker, SynthVoice, TestSource, ThreeBandEq, VuMeter,
};
pub use noise::NoiseDensity;
pub use param::{ParamDecl, ParamHandle, ParamId, ParamQueue, Params};
pub use port::{AudioFormat, DigitalFace, EventFace, InputPort, OutputPort};
pub use readout::{ReadoutDecl, ReadoutHandle, ReadoutId};
pub use rng::Rng;
pub use schedule::{CompileError, EventInputId, EventQueue, Schedule, ScheduleSlot, compile};
pub use signal::{
    AnalogRate, BitDepth, ClockDomainId, Domain, EventBuffer, EventMessage, Lane, SampleBuffer,
    SampleRate, TimedEvent, VoltageBuffer, Volts,
};
pub use transport::Transport;
pub use wav::{WAV_HEADER_LEN, WavError, WavSpec, decode_wav, encode_wav, wav_header};
