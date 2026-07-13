//! A microphone / instrument preamp — a **balanced**, difference-first front-end into a gain stage
//! with the front-panel switches a real preamp carries (PAD, AIR), and an INST/hi-Z input
//! selectable at construction.

use super::Node;
use crate::dsp::Biquad;
use crate::electrical::{InputZ, Ohms, OutputZ, PhantomSupply};
use crate::param::{ParamDecl, ParamId, Params};
use crate::port::{InputPort, OutputPort};
use crate::signal::{AnalogRate, Lane, SampleRate, Volts};

/// A preamp stage with a **balanced front-end**:
/// `out = shelf(clamp(((V+ − V−) · pad) · gain, ±rail)) · powered`, where `shelf` is the
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
/// toggle. Both choices are **balanced** faces (a combo jack's XLR and TRS paths both carry the pair).
///
/// # The balanced front-end
/// The input is a **balanced** pair — one port, two conductor lanes (V+ then V−, the
/// [`BalancedReceiver`](super::BalancedReceiver) convention) — and `process` takes the difference
/// `s = V+ − V−` **first**, before anything else. Ordering is the physics, not a style choice:
/// common-mode rejection only survives **linear** per-leg processing
/// (`f(cm + s/2) − f(cm − s/2) = f(s)` needs linearity), so it must happen upstream of the first
/// nonlinearity. Put the rail clamp first and a 48 V phantom pedestal pins *both* legs to the rail —
/// `clamp(48·g) − clamp(48·g) = 0`, the audio annihilated with no way to recover it downstream
/// (`osku_physics_concepts.md` §17). Difference-first is exact at the ideal-CMRR altitude (perfectly
/// symmetric legs); finite CMRR is leg *asymmetry*, deferred with the per-leg-caps topology it would
/// make distinguishable. An **unbalanced** source still plugs straight in: the schedule's grounding
/// edge (a TS plug's sleeve shorts the cold pin) delivers hot = signal, cold = 0, and `s − 0 = s`.
///
/// # The phantom supply
/// The mic input **declares** a +48 V phantom feed ([`Node::phantom_supply`]:
/// [`PHANTOM_VOLTS`](Self::PHANTOM_VOLTS) behind [`PHANTOM_FEED_PER_LEG_OHMS`](Self::PHANTOM_FEED_PER_LEG_OHMS)
/// per leg — IEC 61938 P48), engaged or not per [`with_phantom`](Self::with_phantom). Whether it's
/// engaged is **structural**, like INST: the DC network *is* topology, so the 48V switch recompiles
/// (and the swap can't click — the pedestal cancels at this very front-end in both states). The
/// solve itself lives in `compile`; the declared `InputZ` already lumps the feed network's AC
/// loading, as `InputZ` lumps everything.
///
/// One (balanced, two-lane) input; one output.
pub struct MicPreamp {
    gain: f32,
    rail: f32,
    /// The engaged-PAD linear factor, `10^(PAD_DB/20)` — precomputed at construction.
    pad_factor: f32,
    /// Whether the +48 V phantom feed is switched on — part of the declared DC topology
    /// (see [`with_phantom`](Self::with_phantom)), read by `compile`, never by `process`.
    phantom_engaged: bool,
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
    /// The phantom supply rail: standard +48 V (IEC 61938 "P48").
    pub const PHANTOM_VOLTS: f32 = 48.0;
    /// The P48 per-leg feed resistance: 6.8 kΩ onto each conductor (3.4 kΩ effective for the
    /// common-mode draw — the two legs in parallel).
    pub const PHANTOM_FEED_PER_LEG_OHMS: f32 = 6_800.0;
    /// AIR high-shelf design: corner, Q, and boost. Informed approximation of the published Air curve.
    const AIR_FREQ_HZ: f64 = 10_000.0;
    const AIR_Q: f64 = 0.707;
    const AIR_GAIN_DB: f64 = 4.0;

    /// A preamp with voltage gain `gain`, clipping at `±rail`, presenting `z_in` (the INST/line choice)
    /// and driving from `z_out`. PAD and AIR default off.
    ///
    /// # Panics
    /// Panics unless `rail` is finite and `> 0` — a degenerate clamp is a setup bug — and unless
    /// `z_in` is **balanced** (two conductors): the front-end reads two input lanes, so an unbalanced
    /// face would starve it of the cold leg. Both are caught here at construction, never on the hot
    /// path.
    #[must_use]
    pub fn new(gain: f32, rail: Volts, z_in: InputZ, z_out: Ohms) -> Self {
        let rail = rail.get();
        assert!(
            rail.is_finite() && rail > 0.0,
            "MicPreamp rail must be finite and > 0, got {rail}"
        );
        assert!(
            z_in.conductors() == 2,
            "MicPreamp z_in must be balanced (2 conductors), got {}",
            z_in.conductors()
        );
        Self {
            gain,
            rail,
            pad_factor: 10.0_f32.powf(Self::PAD_DB / 20.0),
            phantom_engaged: false,
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

    /// The same preamp with its +48 V phantom feed switched on or off (off by default from
    /// [`new`](Self::new)). Structural, like the INST impedance choice: the catalog maps the
    /// device's 48V config to this at build, and toggling it recompiles.
    #[must_use]
    pub fn with_phantom(mut self, engaged: bool) -> Self {
        self.phantom_engaged = engaged;
        self
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

    fn phantom_supply(&self, port: usize) -> Option<PhantomSupply> {
        // The feed network is declared engaged or not — the 48V switch is part of the topology,
        // and `compile` resolves whatever load faces this input against it.
        (port == 0).then(|| {
            PhantomSupply::new(
                Volts::new(Self::PHANTOM_VOLTS),
                Ohms::new(Self::PHANTOM_FEED_PER_LEG_OHMS),
                self.phantom_engaged,
            )
        })
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
        // tests). Chain: difference (V+ − V−, FIRST — see the front-end doc: rejection must precede
        // the clamp) → PAD (pre-gain) → gain → rail clip → AIR crossfade → power gate (last, like
        // `GainStage`, so an off preamp passes nothing).
        let (fallback, rail, pad_factor) = (self.gain, self.rail, self.pad_factor);
        // One balanced input port = two conductor lanes: [0] = V+, [1] = V−.
        let vp = inputs[0].voltage().as_slice();
        let vn = inputs[1].voltage().as_slice();
        let out = outputs[0].voltage_mut().as_mut_slice();
        match &mut self.air {
            Some(shelf) => {
                for (i, (o, (&p, &n))) in out.iter_mut().zip(vp.iter().zip(vn)).enumerate() {
                    let gain = params.value_at_or(Self::GAIN, i, fallback);
                    let pad = params.value_at_or(Self::PAD, i, 0.0);
                    let air = params.value_at_or(Self::AIR, i, 0.0);
                    let powered = params.value_at_or(Self::POWERED, i, 1.0);
                    // Difference-first: anything common-mode (phantom pedestal, coupled hum)
                    // cancels here, while it's still linear territory.
                    let v = p - n;
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
                for (i, (o, (&p, &n))) in out.iter_mut().zip(vp.iter().zip(vn)).enumerate() {
                    let gain = params.value_at_or(Self::GAIN, i, fallback);
                    let pad = params.value_at_or(Self::PAD, i, 0.0);
                    let powered = params.value_at_or(Self::POWERED, i, 1.0);
                    let padded = (p - n) * (1.0 + pad * (pad_factor - 1.0));
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
            InputZ::balanced(Ohms::new(10_000.0)),
            Ohms::new(150.0),
        )
    }

    /// A hot-driven balanced input pair: V+ carries `hot`, V− is grounded 0 V — the shape the
    /// schedule's grounding edge delivers for an unbalanced source, so `V+ − V− = hot` and each
    /// single-ended test keeps its exact hand numbers.
    fn hot_and_grounded_cold(hot: &[f32]) -> [VoltageBuffer; 2] {
        [
            VoltageBuffer::from_volts(hot.to_vec(), rate()),
            VoltageBuffer::zeros(hot.len(), rate()),
        ]
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
        // INST/line is the constructed input impedance — the face reflects exactly what was passed:
        // a balanced (two-conductor) input of the given differential Z, an unbalanced output.
        let inst = MicPreamp::new(
            1.0,
            Volts::new(10.0),
            InputZ::balanced(Ohms::new(1_500_000.0)),
            Ohms::new(150.0),
        );
        assert_eq!(
            inst.inputs(),
            &[InputPort::Analog(InputZ::balanced(Ohms::new(1_500_000.0)))]
        );
        assert_eq!(inst.inputs()[0].lane_count(), 2);
        assert_eq!(
            inst.outputs(),
            &[OutputPort::Analog(OutputZ::new(Ohms::new(150.0)))]
        );
    }

    #[test]
    fn declares_the_phantom_supply_disengaged_by_default() {
        // The P48 feed network is a circuit-topology declaration on the mic input: +48 V behind
        // 6.8 kΩ per leg. `new` leaves it disengaged; `with_phantom(true)` is the 48V switch.
        let p = preamp(1.0, 10.0);
        let supply = p.phantom_supply(0).expect("input 0 declares the feed");
        assert_relative_eq!(supply.volts().get(), 48.0);
        assert_relative_eq!(supply.feed_per_leg().get(), 6_800.0);
        assert!(!supply.engaged(), "disengaged until the 48V switch");
        assert!(p.phantom_supply(1).is_none(), "only the mic input feeds");

        let engaged = preamp(1.0, 10.0).with_phantom(true);
        assert!(engaged.phantom_supply(0).expect("declared").engaged());
    }

    #[test]
    #[should_panic(expected = "must be balanced")]
    fn rejects_an_unbalanced_input_face() {
        // The front-end reads two lanes; an unbalanced face would starve it of the cold leg —
        // a construction bug, caught at construction rather than on the hot path.
        let _ = MicPreamp::new(
            1.0,
            Volts::new(10.0),
            InputZ::new(Ohms::new(10_000.0)),
            Ohms::new(150.0),
        );
    }

    #[test]
    fn applies_gain_below_the_rail() {
        // Hot 0.5 V, cold grounded → s = 0.5 V; × 4 = 2.0 V, under a 10 V rail → linear (PAD/AIR
        // off, run outside a schedule).
        let mut p = preamp(4.0, 10.0);
        p.prepare(rate());
        let input = hot_and_grounded_cold(&[0.5_f32; 64]);
        let mut out = [VoltageBuffer::zeros(64, rate())];
        process_voltage(&mut p, &input, &mut out);
        // AIR is off (default 0), so the shelf crossfade contributes nothing: pure gain.
        assert!(out[0].as_slice().iter().all(|&v| (v - 2.0).abs() < 1e-6));
    }

    #[test]
    fn clips_hard_at_the_rail() {
        // Hot ±0.5 V (cold grounded) × 4 wants ±2.0 V but the rail is 1.5 V → clamps; symmetric
        // hard clip in volts.
        let mut p = preamp(4.0, 1.5);
        p.prepare(rate());
        let input = hot_and_grounded_cold(&[0.5, -0.5]);
        let mut out = [VoltageBuffer::zeros(2, rate())];
        process_voltage(&mut p, &input, &mut out);
        assert_relative_eq!(out[0].get(0).get(), 1.5, epsilon = 1e-6);
        assert_relative_eq!(out[0].get(1).get(), -1.5, epsilon = 1e-6);
    }

    #[test]
    fn pedestal_rejected_before_the_clamp() {
        // The story's headline oracle: a 48 V phantom pedestal with a 10 mV differential signal —
        // V+ = 48.005 V, V− = 47.995 V — through gain 100 under a ±10 V rail comes out as
        //   (V+ − V−) · gain = 0.01 · 100 = 1.0 V,   with zero DC (no trace of the 48 V).
        // Ordering is what this proves: were the clamp applied per leg *before* the difference,
        // clamp(48.005·100, ±10) − clamp(47.995·100, ±10) = 10 − 10 = 0 — both legs pin to the
        // rail and the audio is annihilated (physics §17). Difference-first makes the pedestal
        // vanish while everything is still linear.
        let mut p = preamp(100.0, 10.0);
        p.prepare(rate());
        let ins = [
            VoltageBuffer::from_volts(vec![48.005_f32; 64], rate()),
            VoltageBuffer::from_volts(vec![47.995_f32; 64], rate()),
        ];
        let mut out = [VoltageBuffer::zeros(64, rate())];
        process_voltage(&mut p, &ins, &mut out);
        for &v in out[0].as_slice() {
            // f32 note: 48.005 − 47.995 carries ~µV-scale rounding at this magnitude, ×100 ≈ mV.
            assert_relative_eq!(v, 1.0, epsilon = 1e-3);
        }
    }

    #[test]
    fn common_mode_hum_cancels_at_the_front_end() {
        // A differential 0.2 V signal (V± = ±0.1 V) with the *identical* hum ramp added to both
        // legs: V+ = 0.1 + h_i, V− = −0.1 + h_i. The difference is (0.1 + h) − (−0.1 + h) = 0.2 V
        // for every sample — the hum cancels exactly (ideal CMRR), so out = 0.2 · 4 = 0.8 V flat.
        let mut p = preamp(4.0, 10.0);
        p.prepare(rate());
        let n = 64;
        let hum = |i: usize| (i as f32 / n as f32) - 0.5; // a ±0.5 V common-mode sweep
        let ins = [
            VoltageBuffer::from_volts((0..n).map(|i| 0.1 + hum(i)).collect(), rate()),
            VoltageBuffer::from_volts((0..n).map(|i| -0.1 + hum(i)).collect(), rate()),
        ];
        let mut out = [VoltageBuffer::zeros(n, rate())];
        process_voltage(&mut p, &ins, &mut out);
        for &v in out[0].as_slice() {
            assert_relative_eq!(v, 0.8, epsilon = 1e-5);
        }
    }

    #[test]
    fn pad_attenuates_by_ten_db() {
        // PAD engaged scales the differential input by 10^(−10/20) ≈ 0.3162 before the (unity)
        // gain. Hand calc: hot 1.0 V (cold grounded) → s = 1.0 V × 0.31623 × 1 = 0.31623 V.
        // Compared to PAD off (1.0 V), that's exactly −10 dB.
        let mut p = preamp(1.0, 10.0);
        p.prepare(rate());
        let [hot, cold] = hot_and_grounded_cold(&[1.0_f32; 32]);
        let inp = [Lane::Voltage(hot), Lane::Voltage(cold)];

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
                // Hot carries the tone, cold is grounded — the unbalanced-source shape.
                let inp = [
                    Lane::Voltage(buf.clone()),
                    Lane::Voltage(VoltageBuffer::zeros(buf.len(), buf.rate())),
                ];
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
            let inp = [
                Lane::Voltage(buf.clone()),
                Lane::Voltage(VoltageBuffer::zeros(buf.len(), buf.rate())),
            ];
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
        let [hot, cold] = hot_and_grounded_cold(&[0.5_f32; 32]);
        let inp = [Lane::Voltage(hot), Lane::Voltage(cold)];
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
