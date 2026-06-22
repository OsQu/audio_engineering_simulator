//! The offline render driver: drive the engine block by block and capture the tapped speaker
//! voltage into host samples.
//!
//! This is emphatically **not a second engine** — it loops [`Schedule::process_with_events`] (the
//! one real code path) and feeds each block's output tap through the [`Capture`]. The schedule's
//! output tap is expected to be the speaker's voltage (Story 2.1); what comes back is mono,
//! normalized host audio ready for [`crate::wav`].

use crate::capture::Capture;
use engine::{AnalogRate, EventQueue, SampleRate, Schedule, VoltageBuffer};

/// What to render and how long.
pub struct RenderConfig {
    /// The host output sample rate (must integer-divide the analog rate this epic).
    pub host_rate: SampleRate,
    /// Speaker volts that map to digital full scale (±1.0) — the fixed monitor reference.
    pub full_scale_volts: f32,
    /// Render length in seconds (of host audio).
    pub seconds: f64,
}

/// Render `cfg.seconds` of `schedule` to mono host samples (±1.0), delivering `events` as the
/// timeline advances. `analog_rate` is the rate the schedule was compiled at.
///
/// The returned vector holds exactly `round(host_rate · seconds)` samples. The render carries the
/// fixed group delay of the capture FIR (and of any modeled AD/DA in the patch) as latency at the
/// front — fine for listening; the validation oracle offsets for it.
///
/// # Panics
/// Panics unless the schedule's `block_len` is a multiple of the capture's decimation factor, or
/// via [`Capture::new`] (non-integer rate ratio / bad full scale).
#[must_use]
pub fn render_to_samples(
    schedule: &mut Schedule,
    analog_rate: AnalogRate,
    events: &mut EventQueue,
    cfg: &RenderConfig,
) -> Vec<f32> {
    let block_len = schedule.block_len();
    let mut capture = Capture::new(analog_rate, cfg.host_rate, cfg.full_scale_volts);
    assert_eq!(
        block_len % capture.factor(),
        0,
        "schedule block_len ({block_len}) must be a multiple of the capture factor ({})",
        capture.factor(),
    );

    let host_per_block = capture.host_len(block_len);
    let total_host = (cfg.host_rate.as_hz() * cfg.seconds).round() as usize;
    let n_blocks = total_host.div_ceil(host_per_block.max(1));

    let mut out = VoltageBuffer::zeros(block_len, analog_rate);
    let mut host_block = vec![0.0_f32; host_per_block];
    let mut samples = Vec::with_capacity(n_blocks * host_per_block);
    for _ in 0..n_blocks {
        schedule.process_with_events(&mut out, events);
        capture.process(out.as_slice(), &mut host_block);
        samples.extend_from_slice(&host_block);
    }
    samples.truncate(total_host);
    samples
}
