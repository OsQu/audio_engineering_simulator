//! A microphone / instrument preamp — a gain stage with the front-panel switches a real preamp
//! carries (PAD, AIR), and an INST/hi-Z input selectable at construction.

use super::Node;
use crate::dsp::Biquad;
use crate::electrical::{InputZ, Ohms, OutputZ};
use crate::param::{ParamDecl, ParamId, Params};
use crate::port::{InputPort, OutputPort};
use crate::signal::{AnalogRate, Lane, SampleRate, Volts};

/// A preamp stage: `out = shelf(clamp((in · pad) · gain, ±rail)) · powered`, where `shelf` is the
/// smoothly-engaged AIR high-shelf.
///
/// Modeled on [`GainStage`](super::GainStage) (a buffered active stage — real `InputZ` in, `OutputZ`
/// out, a voltage gain and a supply rail the output clips against) plus the analog-filter machinery of
/// [`DcBlocker`](super::DcBlocker) (a filter whose coefficients are baked at
/// [`prepare`](Node::prepare) from the analog rate). It adds the three switches a mic preamp exposes:
///
/// - **PAD** ([`PAD`](Self::PAD)) — a fixed input attenuation ([`PAD_DB`](Self::PAD_DB), ≈ −10 dB)
///   applied *before* the gain, for hot sources. A smoothed 0/1 param interpolates the linear
///   attenuation factor, so toggling is click-free.
/// - **AIR** ([`AIR`](Self::AIR)) — Focusrite's "Air" presence lift, modeled as an analog
///   **high-shelf** ([`Biquad::high_shelf`] fed the *analog* rate). The smoothed 0/1 param
///   **crossfades** the dry and shelved signals rather than switching filter coefficients, so the
///   transition is glitch-free; the biquad runs every sample (state stays warm) regardless. Its
///   corner/gain (≈ +4 dB @ 10 kHz) are an **informed approximation** of the published Air curve.
/// - **POWERED** ([`POWERED`](Self::POWERED)) — the output gate, decl-identical to
///   [`GainStage::POWERED`](super::GainStage::POWERED) so a device-level power group can drive both.
///
/// **INST / hi-Z** is *not* a param: it selects the preamp's input impedance, which the loading
/// divider bakes at compile — a structural choice. So it is a **constructor argument** (`z_in`); the
/// catalog picks a line-level vs instrument-level `InputZ` from the device config and rebuilds on
/// toggle. This node is single-ended (one conductor), like `GainStage`; a balanced front-end (and the
/// per-conductor lift that would come with it) is deferred with the rest of the balanced-preamp model.
///
/// One input; one output.
pub struct MicPreamp {
    gain: f32,
    rail: f32,
    /// The engaged-PAD linear factor, `10^(PAD_DB/20)` — precomputed at construction.
    pad_factor: f32,
    /// The AIR high-shelf, baked from the analog rate at [`prepare`](Node::prepare). `None` until
    /// prepared — an unprepared preamp applies no shelf (AIR inert), like `DcBlocker`.
    air: Option<Biquad>,
    /// The declared control params: [`GAIN`](Self::GAIN), [`POWERED`](Self::POWERED),
    /// [`PAD`](Self::PAD), [`AIR`](Self::AIR).
    param_decls: [ParamDecl; 4],
    inputs: [InputPort; 1],
    outputs: [OutputPort; 1],
}

impl MicPreamp {
    /// The smoothed voltage-gain control. Uncontrolled, it holds the construction `gain`.
    pub const GAIN: ParamId = ParamId(0);
    /// Power switch (`0` = off, `1` = on) — the output gate. Decl-identical (range/default/smooth) to
    /// [`GainStage::POWERED`](super::GainStage::POWERED) so a device power group can bind both.
    pub const POWERED: ParamId = ParamId(1);
    /// PAD switch (`0` = off, `1` = engaged): a [`PAD_DB`](Self::PAD_DB) input attenuation before the
    /// gain. Smoothed, so the linear factor glides — a click-free toggle.
    pub const PAD: ParamId = ParamId(2);
    /// AIR switch (`0` = off, `1` = engaged): crossfades in the high-shelf presence lift. Smoothed.
    pub const AIR: ParamId = ParamId(3);

    /// Largest controllable gain (≈ +60 dB) and the de-zipper glide time — matching `GainStage`.
    const MAX_GAIN: f32 = 1000.0;
    const GAIN_SMOOTH_MS: f32 = 5.0;
    /// De-click glide for the switch params (power/pad/air) — short enough to feel instant.
    const SWITCH_SMOOTH_MS: f32 = 5.0;

    /// PAD attenuation in dB (an informed approximation of the Scarlett pad).
    pub const PAD_DB: f32 = -10.0;
    /// AIR high-shelf design: corner, Q, and boost. Informed approximation of the published Air curve.
    const AIR_FREQ_HZ: f64 = 10_000.0;
    const AIR_Q: f64 = 0.707;
    const AIR_GAIN_DB: f64 = 4.0;

    /// A preamp with voltage gain `gain`, clipping at `±rail`, presenting `z_in` (the INST/line choice)
    /// and driving from `z_out`. PAD and AIR default off.
    ///
    /// # Panics
    /// Panics unless `rail` is finite and `> 0` — a degenerate clamp is a setup bug, caught here at
    /// construction, never on the hot path.
    #[must_use]
    pub fn new(gain: f32, rail: Volts, z_in: InputZ, z_out: Ohms) -> Self {
        let rail = rail.get();
        assert!(
            rail.is_finite() && rail > 0.0,
            "MicPreamp rail must be finite and > 0, got {rail}"
        );
        Self {
            gain,
            rail,
            pad_factor: 10.0_f32.powf(Self::PAD_DB / 20.0),
            air: None,
            param_decls: [
                ParamDecl {
                    id: Self::GAIN,
                    default: gain,
                    min: 0.0,
                    max: Self::MAX_GAIN,
                    smooth_ms: Self::GAIN_SMOOTH_MS,
                },
                ParamDecl {
                    id: Self::POWERED,
                    default: 1.0,
                    min: 0.0,
                    max: 1.0,
                    smooth_ms: Self::SWITCH_SMOOTH_MS,
                },
                ParamDecl {
                    id: Self::PAD,
                    default: 0.0,
                    min: 0.0,
                    max: 1.0,
                    smooth_ms: Self::SWITCH_SMOOTH_MS,
                },
                ParamDecl {
                    id: Self::AIR,
                    default: 0.0,
                    min: 0.0,
                    max: 1.0,
                    smooth_ms: Self::SWITCH_SMOOTH_MS,
                },
            ],
            inputs: [z_in.into()],
            outputs: [OutputZ::new(z_out).into()],
        }
    }
}

impl Node for MicPreamp {
    fn inputs(&self) -> &[InputPort] {
        &self.inputs
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    fn params(&self) -> &[ParamDecl] {
        &self.param_decls
    }

    fn prepare(&mut self, rate: AnalogRate) {
        // Design the AIR shelf at the *analog* rate (the shelf lives in the analog domain), off the
        // hot path. `Biquad` designs against a `SampleRate`, so express the analog rate as one.
        self.air = Some(Biquad::high_shelf(
            SampleRate::new(rate.as_hz()),
            Self::AIR_FREQ_HZ,
            Self::AIR_Q,
            Self::AIR_GAIN_DB,
        ));
    }

    fn process(&mut self, params: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
        // Read gain/pad/air/power per sample so a control change de-zippers across the block; the
        // fallbacks (construction gain, pad/air off, powered on) apply only outside a schedule (unit
        // tests). Chain: PAD (pre-gain) → gain → rail clip → AIR crossfade → power gate (last, like
        // `GainStage`, so an off preamp passes nothing).
        let (fallback, rail, pad_factor) = (self.gain, self.rail, self.pad_factor);
        let src = inputs[0].voltage().as_slice();
        let out = outputs[0].voltage_mut().as_mut_slice();
        match &mut self.air {
            Some(shelf) => {
                for (i, (o, &v)) in out.iter_mut().zip(src).enumerate() {
                    let gain = params.value_at_or(Self::GAIN, i, fallback);
                    let pad = params.value_at_or(Self::PAD, i, 0.0);
                    let air = params.value_at_or(Self::AIR, i, 0.0);
                    let powered = params.value_at_or(Self::POWERED, i, 1.0);
                    // PAD interpolates the linear attenuation: 1.0 (off) → pad_factor (engaged).
                    let padded = v * (1.0 + pad * (pad_factor - 1.0));
                    let amped = (padded * gain).clamp(-rail, rail);
                    // AIR: always step the shelf (state stays warm), crossfade dry → shelved.
                    let shelved = shelf.step(f64::from(amped)) as f32;
                    *o = (amped + air * (shelved - amped)) * powered;
                }
            }
            None => {
                // Unprepared (no rate yet): no shelf. `compile` always prepares before `process`, so a
                // compiled schedule never hits this arm.
                for (i, (o, &v)) in out.iter_mut().zip(src).enumerate() {
                    let gain = params.value_at_or(Self::GAIN, i, fallback);
                    let pad = params.value_at_or(Self::PAD, i, 0.0);
                    let powered = params.value_at_or(Self::POWERED, i, 1.0);
                    let padded = v * (1.0 + pad * (pad_factor - 1.0));
                    *o = (padded * gain).clamp(-rail, rail) * powered;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::param::Smoother;
    use crate::signal::VoltageBuffer;
    use crate::test_util::{measure_gain, process_voltage};
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    fn preamp(gain: f32, rail: f32) -> MicPreamp {
        MicPreamp::new(
            gain,
            Volts::new(rail),
            InputZ::new(Ohms::new(10_000.0)),
            Ohms::new(150.0),
        )
    }

    /// A settled `Params` over the preamp's four params (in id order): gain, powered, pad, air.
    fn params(gain: f32, powered: f32, pad: f32, air: f32) -> [Smoother; 4] {
        [
            Smoother::new(gain, 0.0, MicPreamp::MAX_GAIN, 1.0),
            Smoother::new(powered, 0.0, 1.0, 1.0),
            Smoother::new(pad, 0.0, 1.0, 1.0),
            Smoother::new(air, 0.0, 1.0, 1.0),
        ]
    }

    #[test]
    fn declares_the_constructed_impedances() {
        // INST/line is the constructed input impedance — the face reflects exactly what was passed.
        let inst = MicPreamp::new(
            1.0,
            Volts::new(10.0),
            InputZ::new(Ohms::new(1_500_000.0)),
            Ohms::new(150.0),
        );
        assert_eq!(
            inst.inputs(),
            &[InputPort::Analog(InputZ::new(Ohms::new(1_500_000.0)))]
        );
        assert_eq!(
            inst.outputs(),
            &[OutputPort::Analog(OutputZ::new(Ohms::new(150.0)))]
        );
    }

    #[test]
    fn applies_gain_below_the_rail() {
        // 0.5 V × 4 = 2.0 V, under a 10 V rail → linear (PAD/AIR off, run outside a schedule).
        let mut p = preamp(4.0, 10.0);
        p.prepare(rate());
        let mut input = [VoltageBuffer::zeros(64, rate())];
        input[0].fill(Volts::new(0.5));
        let mut out = [VoltageBuffer::zeros(64, rate())];
        process_voltage(&mut p, &input, &mut out);
        // AIR is off (default 0), so the shelf crossfade contributes nothing: pure gain.
        assert!(out[0].as_slice().iter().all(|&v| (v - 2.0).abs() < 1e-6));
    }

    #[test]
    fn clips_hard_at_the_rail() {
        // 0.5 V × 4 wants 2.0 V but the rail is 1.5 V → clamps; symmetric hard clip in volts.
        let mut p = preamp(4.0, 1.5);
        p.prepare(rate());
        let mut input = [VoltageBuffer::zeros(2, rate())];
        input[0].set(0, Volts::new(0.5));
        input[0].set(1, Volts::new(-0.5));
        let mut out = [VoltageBuffer::zeros(2, rate())];
        process_voltage(&mut p, &input, &mut out);
        assert_relative_eq!(out[0].get(0).get(), 1.5, epsilon = 1e-6);
        assert_relative_eq!(out[0].get(1).get(), -1.5, epsilon = 1e-6);
    }

    #[test]
    fn pad_attenuates_by_ten_db() {
        // PAD engaged scales the input by 10^(−10/20) ≈ 0.3162 before the (unity) gain. Hand calc:
        // 1.0 V × 0.31623 × 1 = 0.31623 V. Compared to PAD off (1.0 V), that's exactly −10 dB.
        let mut p = preamp(1.0, 10.0);
        p.prepare(rate());
        let inp = [Lane::Voltage(VoltageBuffer::from_volts(
            vec![1.0_f32; 32],
            rate(),
        ))];

        let smoothers = params(1.0, 1.0, 1.0, 0.0); // pad engaged
        let mut out = [Lane::Voltage(VoltageBuffer::zeros(32, rate()))];
        p.process(&Params::new(&smoothers), &inp, &mut out);
        let padded = out[0].voltage().get(0).get();
        assert_relative_eq!(padded, 0.316_227_77, epsilon = 1e-5);
        // Ratio to the un-padded 1.0 V input is −10 dB.
        assert_relative_eq!(20.0 * padded.log10(), MicPreamp::PAD_DB, epsilon = 1e-3);
    }

    #[test]
    fn air_lifts_highs_and_leaves_lows() {
        // AIR engaged is a +4 dB high-shelf at 10 kHz: a 20 kHz tone sees ≈ +4 dB (×1.585) over the
        // unity gain; a 100 Hz tone sees ≈ unity. Gain 1, high rail (no clip), measured settled.
        let smoothers = params(1.0, 1.0, 0.0, 1.0); // air engaged
        let with_air = |freq: f64| {
            let mut p = preamp(1.0, 100.0);
            p.prepare(rate());
            measure_gain(freq, rate(), |buf| {
                let inp = [Lane::Voltage(buf.clone())];
                let mut out = [Lane::Voltage(VoltageBuffer::zeros(buf.len(), buf.rate()))];
                p.process(&Params::new(&smoothers), &inp, &mut out);
                buf.as_mut_slice()
                    .copy_from_slice(out[0].voltage().as_slice());
            })
        };
        let g_high = with_air(20_000.0);
        assert_relative_eq!(g_high, 10.0_f32.powf(4.0 / 20.0), epsilon = 0.05);
        let g_low = with_air(100.0);
        assert!(
            (g_low - 1.0).abs() < 0.05,
            "lows should be untouched by the AIR high-shelf, got {g_low}"
        );
    }

    #[test]
    fn air_off_is_transparent() {
        // With AIR off (default), the shelf crossfade contributes nothing at any frequency.
        let smoothers = params(1.0, 1.0, 0.0, 0.0);
        let mut p = preamp(1.0, 100.0);
        p.prepare(rate());
        let g = measure_gain(20_000.0, rate(), |buf| {
            let inp = [Lane::Voltage(buf.clone())];
            let mut out = [Lane::Voltage(VoltageBuffer::zeros(buf.len(), buf.rate()))];
            p.process(&Params::new(&smoothers), &inp, &mut out);
            buf.as_mut_slice()
                .copy_from_slice(out[0].voltage().as_slice());
        });
        assert_relative_eq!(g, 1.0, epsilon = 1e-3);
    }

    #[test]
    fn powered_off_silences_output() {
        let mut p = preamp(4.0, 10.0);
        p.prepare(rate());
        let inp = [Lane::Voltage(VoltageBuffer::from_volts(
            vec![0.5_f32; 32],
            rate(),
        ))];
        let smoothers = params(4.0, 0.0, 0.0, 0.0); // powered off
        let mut out = [Lane::Voltage(VoltageBuffer::zeros(32, rate()))];
        p.process(&Params::new(&smoothers), &inp, &mut out);
        assert!(out[0].voltage().as_slice().iter().all(|&v| v == 0.0));
    }

    #[test]
    #[should_panic(expected = "rail must be finite and > 0")]
    fn rejects_nonpositive_rail() {
        let _ = preamp(1.0, 0.0);
    }
}
