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
//! There is no oversample factor and no digital sample rate here: the analog domain has
//! exactly one clock, [`AnalogRate`]. Digital rates emerge per-converter at the AD/DA
//! boundary (Story 1.6), and [`VoltageBuffer`] is single-conductor until balanced lines
//! arrive (Story 1.5).

mod buffer;
mod rate;
mod volts;

pub use buffer::VoltageBuffer;
pub use rate::AnalogRate;
pub use volts::Volts;
