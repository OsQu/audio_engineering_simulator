//! Core analog-domain signal types.
//!
//! **Scalar policy.** Storage is `f32` — compact and SIMD-friendly for the oversampled
//! hot path. Reach for `f64` only in *accumulators* (filter state, summing nodes, the
//! future AD anti-alias filter) where rounding error would otherwise build up.
//!
//! **Linear only.** Buffers and [`Volts`] hold linear values. Decibels (dBu/dBV, and
//! later dBFS) are *measurement units* produced by conversion helpers — never a storage
//! format. Everything is derived from the physical (volts) model.
//!
//! The analog domain has exactly one clock, [`AnalogRate`]. The digital domain is a separate
//! carrier ([`SampleBuffer`], at a per-converter [`SampleRate`] / [`BitDepth`] / [`ClockDomainId`])
//! that only meets the analog one at an AD/DA — the two are distinct newtypes the type system
//! keeps apart (Story 1.6). [`VoltageBuffer`] is per-conductor (balanced lines, Story 1.5);
//! [`SampleBuffer`] is per-channel.

mod bit_depth;
mod buffer;
mod event;
mod lane;
mod rate;
mod sample;
mod sample_rate;
mod volts;

pub use bit_depth::BitDepth;
pub use buffer::VoltageBuffer;
pub use event::{EventBuffer, EventMessage, TimedEvent};
pub use lane::{Domain, Lane};
pub use rate::AnalogRate;
pub use sample::{ClockDomainId, SampleBuffer};
pub use sample_rate::SampleRate;
pub use volts::Volts;
