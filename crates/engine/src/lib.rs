//! Core voltage engine.
//!
//! The analog domain is modeled as a real, oversampled voltage waveform in volts —
//! physical behavior (levels, impedance loss, clipping, noise, DC, hum) emerges from
//! the voltage math rather than being flagged. See `PROJECT_PLAN.md` and
//! `IMPLEMENTATION_PLAN.md` for the design; this crate stays portable to `wasm32`
//! (no `std::thread`, no ambient `std::time`).
//!
//! Numeric types (`Volts`, `VoltageBuffer`, `AnalogRate`) land in Task 1.1.2.
