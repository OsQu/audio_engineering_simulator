//! A **played note travels the full chain** `analog → AD → digital → DA → analog` — the end-to-end
//! "play an instrument" path. The voice is driven by the event lane (note-on at a chosen sample) and
//! a smoothed control param (level); the converters carry it across the digital domain and back.
//! (The swept-param de-zipper gate is also proven in [`param_phenomena`] on a clean DC signal; here
//! we show it survives end-to-end on the voice.) Tests are the oracle — the fundamental level is a
//! hand calc, inline.

use crate::support::{rms, tone_amplitude};
use approx::assert_relative_eq;
use engine::{
    AdConverter, AnalogRate, BitDepth, DaConverter, EventInputId, EventMessage, EventQueue, Graph,
    Ohms, ParamQueue, SampleRate, Schedule, SynthVoice, VoltageBuffer, Volts, compile,
};

fn analog_rate() -> AnalogRate {
    AnalogRate::new(384_000.0)
}
fn digital_rate() -> SampleRate {
    SampleRate::new(48_000.0)
}

/// `voice → AD → DA → analog tap`, all near-ideal analog faces. Returns the schedule and the
/// voice's event-input handle.
fn voice_through_converters(block: usize) -> (Schedule, EventInputId) {
    let mut g = Graph::new();
    let voice = g.add(SynthVoice::new(Volts::new(1.0), Ohms::new(1.0)));
    let ad = g.add(AdConverter::new(
        digital_rate(),
        BitDepth::new(24),
        Volts::new(10.0),
        Ohms::new(1e6),
    ));
    let da = g.add(DaConverter::new(
        digital_rate(),
        BitDepth::new(24),
        Volts::new(10.0),
        Ohms::new(150.0),
    ));
    g.connect_ideal(voice, 0, ad, 0);
    g.connect_ideal(ad, 0, da, 0);
    g.set_output(da, 0);
    let sched = compile(g, block, analog_rate(), 0).expect("valid playable chain");
    let ev = sched.event_input(voice, 0).expect("voice event input");
    (sched, ev)
}

#[test]
fn a_played_note_travels_analog_ad_digital_da_analog() {
    // Play A4 (note 69 = 440 Hz) and recover it after the round trip. The voice's default
    // sustain 0.7 and level 1.0 V make the analog sawtooth's fundamental
    //   (2/π)·sustain·level = 0.63662·0.7·1.0 = 0.4456 V.
    // 440 Hz sits deep in the 24 kHz passband, so the AD/DA pass it at unity ⇒ the output
    // fundamental is ≈ that, and the AD's anti-alias filter has quietly removed the saw's
    // ultrasonic harmonics that would otherwise fold (the oversampled-oscillator payoff).
    let block = 15_360; // 1920 digital samples — many 440 Hz cycles
    let (mut sched, ev) = voice_through_converters(block);
    let mut q = EventQueue::with_capacity(4);
    q.push(
        0,
        ev,
        EventMessage::NoteOn {
            note: 69,
            velocity: 100,
        },
    );
    let mut out = VoltageBuffer::zeros(block, analog_rate());
    sched.process_with_events(&mut out, &mut q);

    // Read a steady window of ~whole cycles from after the attack + combined converter group
    // delay, so the single-bin DFT lands cleanly on the 440 Hz bin (low leakage).
    let spc = analog_rate().as_hz() / 440.0; // samples per cycle
    let window = (spc * 8.0) as usize; // 8 whole cycles
    let tail = &out.as_slice()[block / 2..block / 2 + window];

    let fundamental = tone_amplitude(tail, 440.0, analog_rate());
    let expected = core::f32::consts::FRAC_2_PI * 0.7 * 1.0;
    assert_relative_eq!(fundamental, expected, max_relative = 0.05);
    // A real pitched note: the fundamental dominates a detuned bin by a wide margin.
    let detuned = tone_amplitude(tail, 550.0, analog_rate());
    assert!(
        fundamental > detuned * 5.0,
        "the note should be a clean 440 Hz tone, not noise ({fundamental} vs {detuned})"
    );
}

#[test]
fn the_chain_is_silent_before_the_note() {
    // Causality across the converters: a note triggered late produces nothing earlier. Filter
    // latency can only *delay* energy, never advance it — and with no input the only thing the
    // AD emits is its sub-µV dither floor, far below any signal.
    let block = 8_192;
    let (mut sched, ev) = voice_through_converters(block);
    let mut q = EventQueue::with_capacity(4);
    let trigger = block as u64 * 3 / 4;
    q.push(
        trigger,
        ev,
        EventMessage::NoteOn {
            note: 69,
            velocity: 100,
        },
    );
    let mut out = VoltageBuffer::zeros(block, analog_rate());
    sched.process_with_events(&mut out, &mut q);

    // The first quarter is well before the trigger (and its latency): silent to the dither floor.
    let head = &out.as_slice()[..block / 4];
    assert!(
        rms(head) < 1e-3,
        "nothing should sound before the note is played, rms {}",
        rms(head)
    );
}

#[test]
fn a_swept_level_de_zippers_on_the_played_voice() {
    // The control-param de-zipper, end-to-end on the voice: hold a high note (so many periods
    // resolve the ramp), then sweep LEVEL 0.3 → 1.2 V (4×, within the 0–1.5 V range). The output's
    // windowed RMS must climb *smoothly* to ≈4× — a raw write would jump it in a single window.
    let block = 8_192;
    let mut g = Graph::new();
    let voice = g.add(SynthVoice::new(Volts::new(1.0), Ohms::new(1.0)));
    g.set_output(voice, 0);
    let mut sched = compile(g, block, analog_rate(), 0).expect("valid voice chain");
    let ev = sched.event_input(voice, 0).expect("voice event input");
    let level = sched.param(voice, SynthVoice::LEVEL).expect("level param");

    // Block 1: establish a sustained C7 (note 96 ≈ 2093 Hz) at a low 0.3 V (settles within the block).
    let mut q = EventQueue::with_capacity(4);
    q.push(
        0,
        ev,
        EventMessage::NoteOn {
            note: 96,
            velocity: 100,
        },
    );
    let mut pq0 = ParamQueue::with_capacity(1);
    pq0.set(level, 0.3);
    let mut out = VoltageBuffer::zeros(block, analog_rate());
    sched.process_io(&mut out, &mut pq0, &mut q);

    // Block 2: aim LEVEL at 1.2 V and capture the glide (it ramps over the 5 ms smooth time).
    let mut pq = ParamQueue::with_capacity(1);
    pq.set(level, 1.2);
    sched.process_with_params(&mut out, &mut pq);

    // Window RMS over ~2 periods (note 96 period ≈ 183 samples). A smooth ramp spreads the
    // rise across many windows; assert it's non-decreasing, lands at ≈4× the start, and no
    // single window jumps by more than a fraction of the total change (rules out a step).
    let win = 366;
    let rms_windows: Vec<f32> = out.as_slice().chunks(win).map(rms).collect();
    let first = rms_windows[0];
    let last = *rms_windows.last().unwrap();
    assert_relative_eq!(last / first, 4.0, max_relative = 0.15);
    assert!(
        rms_windows.windows(2).all(|w| w[1] >= w[0] - 1e-4),
        "the level glide should be monotonic"
    );
    let total = last - first;
    let max_step = rms_windows
        .windows(2)
        .map(|w| w[1] - w[0])
        .fold(0.0_f32, f32::max);
    assert!(
        max_step < total * 0.5,
        "no single window may jump the whole change (max step {max_step} of {total})"
    );
}
