//! A gain / preamp stage.

use super::Node;
use crate::electrical::{InputZ, Ohms, OutputZ};
use crate::noise::NoiseDensity;
use crate::param::{ParamDecl, ParamId, Params};
use crate::port::{InputPort, OutputPort};
use crate::rng::Rng;
use crate::signal::{Lane, Volts};

/// A gain stage with a finite supply rail and an optional input-referred noise floor:
/// `out = clamp((in + n) · gain, ±rail) · powered`.
///
/// Models a buffered active stage — a real `InputZ` it presents to its source and a real
/// output impedance it drives downstream, with a voltage gain in between. The **rail** is the
/// supply voltage the output can't swing past; beyond it the signal clips, hard, in volts.
/// That the rail and clamp live here (not as a flag) is the point: headroom and clipping
/// *emerge* from the physics.
///
/// The optional **noise** `n` is white Gaussian noise referred to the *input* — added before
/// the gain, so amplifying the signal amplifies its own noise too: the stage that sets your
/// SNR is the first gain stage (the preamp), which is the lesson. Its level is a spectral
/// density ([`NoiseDensity`], V/√Hz); the per-sample `σ` follows from the analog rate. A stage
/// built with [`new`](Self::new) is noiseless; [`with_noise`](Self::with_noise) adds a floor.
///
/// One input; one output.
pub struct GainStage {
    gain: f32,
    rail: f32,
    /// Input-referred white-noise floor. [`NoiseDensity::ZERO`] ⇒ noiseless.
    noise_density: NoiseDensity,
    /// The per-node noise stream, installed by [`Node::seed`] at compile when a floor is set.
    noise: Option<Rng>,
    /// The declared control params: [`GAIN`](Self::GAIN) and [`POWERED`](Self::POWERED).
    param_decls: [ParamDecl; 2],
    inputs: [InputPort; 1],
    outputs: [OutputPort; 1],
}

impl GainStage {
    /// The smoothed voltage-gain control param. The host drives it with `(node, GainStage::GAIN)`;
    /// uncontrolled, it holds the construction `gain`.
    pub const GAIN: ParamId = ParamId(0);
    /// Power switch (`0` = off, `1` = on). A powered-off stage **passes nothing** — its output is
    /// gated to silence (noise included). The smoothed value de-clicks the on/off transition, so a
    /// toggle is glitch-free without being a structural graph edit. Defaults on (`1.0`).
    pub const POWERED: ParamId = ParamId(1);

    /// Largest controllable gain (≈ +60 dB) and the de-zipper glide time for a gain change.
    const MAX_GAIN: f32 = 1000.0;
    const GAIN_SMOOTH_MS: f32 = 5.0;
    /// De-click glide for the power switch — short enough to feel instant, long enough to avoid a
    /// click on the output-gate step.
    const POWER_SMOOTH_MS: f32 = 5.0;

    /// A noiseless stage with voltage gain `gain`, clipping at `±rail`, presenting `z_in` and
    /// driving from `z_out`.
    ///
    /// # Panics
    /// Panics unless `rail` is finite and `> 0` — a non-positive or non-finite rail is a setup
    /// bug (it would make the clamp degenerate). Checked here at construction, never on the hot
    /// path.
    #[must_use]
    pub fn new(gain: f32, rail: Volts, z_in: InputZ, z_out: Ohms) -> Self {
        let rail = rail.get();
        assert!(
            rail.is_finite() && rail > 0.0,
            "GainStage rail must be finite and > 0, got {rail}"
        );
        Self {
            gain,
            rail,
            noise_density: NoiseDensity::ZERO,
            noise: None,
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
                    smooth_ms: Self::POWER_SMOOTH_MS,
                },
            ],
            inputs: [z_in.into()],
            outputs: [OutputZ::new(z_out).into()],
        }
    }

    /// Add an input-referred white-noise floor of spectral density `density`.
    ///
    /// The stream is installed at compile via [`Node::seed`], so the noise is reproducible for
    /// a given compile seed. Builder style: `GainStage::new(..).with_noise(density)`.
    #[must_use]
    pub fn with_noise(mut self, density: NoiseDensity) -> Self {
        self.noise_density = density;
        self
    }

    /// Constrain the [`GAIN`](Self::GAIN) control to `[min, max]` (voltage-gain multipliers) instead
    /// of the default `0..MAX_GAIN` (≈ +60 dB). Use for a **volume/level** control — a
    /// monitor or headphone knob that attenuates from unity down to silence rather than boosting like
    /// a preamp. The construction `gain` (the decl default) must lie within the new range.
    ///
    /// # Panics
    /// Panics unless `0.0 <= min < max` (both finite) and the construction gain lies within
    /// `[min, max]` — a setup bug, caught here at construction, never on the hot path.
    #[must_use]
    pub fn with_gain_range(mut self, min: f32, max: f32) -> Self {
        assert!(
            min.is_finite() && max.is_finite() && (0.0..max).contains(&min),
            "GainStage gain range must satisfy 0 <= min < max, got [{min}, {max}]"
        );
        assert!(
            (min..=max).contains(&self.gain),
            "GainStage construction gain {} must lie within [{min}, {max}]",
            self.gain
        );
        // GAIN is decl 0 (POWERED is decl 1), matching `new`.
        self.param_decls[0].min = min;
        self.param_decls[0].max = max;
        self
    }
}

impl Node for GainStage {
    fn inputs(&self) -> &[InputPort] {
        &self.inputs
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    fn params(&self) -> &[ParamDecl] {
        &self.param_decls
    }

    fn seed(&mut self, rng: Rng) {
        // Only keep a stream if there's a floor to draw for; a noiseless stage stays a plain
        // pass-through (and still consumes its split, so streams are stable across the graph).
        if self.noise_density != NoiseDensity::ZERO {
            self.noise = Some(rng);
        }
    }

    fn process(&mut self, params: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
        // Gain and power are read per sample so a control change de-zippers smoothly across the
        // block; `fallback` (the construction gain) and a powered-on `1.0` are used only when run
        // without a schedule (unit tests). Power gates the *output* (after the gain + rail clip), so
        // a powered-off stage passes nothing — including its own noise.
        let (fallback, rail) = (self.gain, self.rail);
        let in_buf = inputs[0].voltage();
        let src = in_buf.as_slice();
        let out = outputs[0].voltage_mut().as_mut_slice();
        match &mut self.noise {
            Some(rng) => {
                // σ = D·√(fs/2) from the block's rate; one √ per block, off the per-sample loop.
                let sigma = self.noise_density.per_sample_sigma(in_buf.rate());
                for (i, (o, &v)) in out.iter_mut().zip(src).enumerate() {
                    let gain = params.value_at_or(Self::GAIN, i, fallback);
                    let powered = params.value_at_or(Self::POWERED, i, 1.0);
                    let n = rng.next_gaussian() * sigma;
                    *o = ((v + n) * gain).clamp(-rail, rail) * powered;
                }
            }
            None => {
                for (i, (o, &v)) in out.iter_mut().zip(src).enumerate() {
                    let gain = params.value_at_or(Self::GAIN, i, fallback);
                    let powered = params.value_at_or(Self::POWERED, i, 1.0);
                    *o = (v * gain).clamp(-rail, rail) * powered;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::{AnalogRate, VoltageBuffer};
    use crate::test_util::process_voltage;
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    fn stage(gain: f32, rail: f32) -> GainStage {
        GainStage::new(
            gain,
            Volts::new(rail),
            InputZ::new(Ohms::new(10_000.0)),
            Ohms::new(150.0),
        )
    }

    #[test]
    fn declares_faces() {
        let s = stage(2.0, 10.0);
        assert_eq!(
            s.inputs(),
            &[InputPort::Analog(InputZ::new(Ohms::new(10_000.0)))]
        );
        assert_eq!(
            s.outputs(),
            &[OutputPort::Analog(OutputZ::new(Ohms::new(150.0)))]
        );
    }

    #[test]
    fn applies_gain_below_the_rail() {
        // 0.5 V × 4 = 2.0 V, well under a 10 V rail → linear.
        let mut s = stage(4.0, 10.0);
        let mut input = [VoltageBuffer::zeros(4, rate())];
        input[0].fill(Volts::new(0.5));
        let mut out = [VoltageBuffer::zeros(4, rate())];
        process_voltage(&mut s, &input, &mut out);
        assert!(out[0].as_slice().iter().all(|&v| (v - 2.0).abs() < 1e-6));
    }

    #[test]
    fn clips_hard_at_the_rail() {
        // 0.5 V × 4 = 2.0 V wanted, but the rail is 1.5 V → clamps to +1.5 V; the negative
        // half clamps to −1.5 V. Symmetric hard clip in volts.
        let mut s = stage(4.0, 1.5);
        let mut input = [VoltageBuffer::zeros(2, rate())];
        input[0].set(0, Volts::new(0.5));
        input[0].set(1, Volts::new(-0.5));
        let mut out = [VoltageBuffer::zeros(2, rate())];
        process_voltage(&mut s, &input, &mut out);
        assert_relative_eq!(out[0].get(0).get(), 1.5, epsilon = 1e-6);
        assert_relative_eq!(out[0].get(1).get(), -1.5, epsilon = 1e-6);
    }

    #[test]
    #[should_panic(expected = "rail must be finite and > 0")]
    fn rejects_nonpositive_rail() {
        let _ = stage(1.0, 0.0);
    }

    #[test]
    fn with_gain_range_overrides_the_gain_decl() {
        // A volume/level control: unity default, capped at unity, attenuating to silence.
        let s = stage(1.0, 10.0).with_gain_range(0.0, 1.0);
        let gain = s
            .params()
            .iter()
            .find(|d| d.id == GainStage::GAIN)
            .expect("GAIN decl");
        assert_eq!(gain.min, 0.0);
        assert_eq!(gain.max, 1.0);
        assert_eq!(gain.default, 1.0, "the construction gain stays the default");
    }

    #[test]
    #[should_panic(expected = "gain range must satisfy 0 <= min < max")]
    fn rejects_an_inverted_gain_range() {
        let _ = stage(1.0, 10.0).with_gain_range(1.0, 0.5);
    }

    #[test]
    #[should_panic(expected = "must lie within")]
    fn rejects_a_default_outside_the_gain_range() {
        // Construction gain 4.0 with a unity ceiling is a setup bug.
        let _ = stage(4.0, 10.0).with_gain_range(0.0, 1.0);
    }
}
