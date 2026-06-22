//! Digital DSP primitives — the digital-domain peers of the analog electrical filters.
//!
//! Where [`electrical`](crate::electrical) holds the analog wire physics (one-pole cable rolloff,
//! DC blocker), this module holds the building blocks of digital processing that runs *after* the
//! AD converter: the [`Biquad`] second-order filter and (later) the dynamics primitives. They share
//! the hot-path contract of the analog filters — `f64` state, zero-alloc, panic-free, denormals
//! flushed — and the same [`flush_denormal`] helper, which lives here because a recursive filter in
//! either domain needs it.

mod biquad;
mod envelope;

pub use biquad::Biquad;
pub use envelope::PeakEnvelope;

/// Flush a subnormal `f64` to zero.
///
/// Subnormals (below [`f64::MIN_POSITIVE`]) can trap the FPU into slow microcode — fatal in a
/// real-time worklet — and any recursive filter's decaying tail drifts into them. Anything that
/// small is silence, so snapping it to zero is free of audible cost. Shared by the analog
/// [`OnePole`](crate::OnePole) and the digital [`Biquad`] (both feed state back every sample).
#[inline]
pub(crate) fn flush_denormal(x: f64) -> f64 {
    if x.abs() < f64::MIN_POSITIVE { 0.0 } else { x }
}
