//! A balanced electro-optical-style line leveler.

use super::Node;
use crate::dsp::flush_denormal;
use crate::electrical::{InputZ, Ohms, OutputZ};
use crate::level::volts_to_dbu;
use crate::param::{ParamDecl, ParamId, Params};
use crate::port::{InputPort, OutputPort};
use crate::readout::{ReadoutDecl, ReadoutId};
use crate::signal::{AnalogRate, Lane, Volts};

/// A mono balanced analog leveler with an intentionally small, legible optical-style model.
///
/// The detector always sees the differential input `V+ − V−`, before compression or output clipping.
/// That ordering preserves ideal common-mode rejection; the processed differential output is then split
/// equally and oppositely across the two output conductors. It is a voltage-domain line processor — no
/// converter or digital carrier is involved.
pub struct LumaLeveler {
    detector_coeff_attack: f64,
    detector_coeff_release: f64,
    output_meter_coeff: f64,
    detector: f64,
    output_mean_square: f64,
    gain_reduction_db: f32,
    output_dbu: f32,
    param_decls: [ParamDecl; 4],
    readouts: [ReadoutDecl; 2],
    inputs: [InputPort; 1],
    outputs: [OutputPort; 1],
}

impl LumaLeveler {
    /// Peak Reduction is a unitless front-panel drive control: 0 leaves a high threshold; 1 reaches
    /// the low threshold where the model can show approximately 40 dB of reduction.
    pub const PEAK_REDUCTION: ParamId = ParamId(0);
    /// Post-reduction voltage gain: 1.0 is unity, 100 is +40 dB.
    pub const GAIN: ParamId = ParamId(1);
    /// 0 selects the gentle Compress ratio; 1 selects the firmer Limit ratio.
    pub const LIMIT: ParamId = ParamId(2);
    /// Smoothed output gate; this chassis has no bypass path.
    pub const POWERED: ParamId = ParamId(3);

    /// Current downward gain reduction, expressed as a positive number of dB.
    pub const GAIN_REDUCTION_DB: ReadoutId = ReadoutId(0);
    /// Ballistic differential output level, in dBu RMS.
    pub const OUTPUT_DBU: ReadoutId = ReadoutId(1);

    const LINE_Z_OHMS: f32 = 600.0;
    const OUTPUT_RAIL_PER_LEG_V: f32 = 20.0;
    const PEAK_REDUCTION_DEFAULT: f32 = 0.0;
    const GAIN_DEFAULT: f32 = 1.0;
    const ATTACK_MS: f64 = 10.0;
    const RELEASE_MS: f64 = 500.0;
    const OUTPUT_METER_MS: f64 = 65.0;
    const KNEE_DB: f64 = 6.0;
    const COMPRESS_RATIO: f64 = 3.0;
    const LIMIT_RATIO: f64 = 20.0;
    const MAX_REDUCTION_DB: f64 = 40.0;
    const READING_FLOOR_DBU: f32 = -60.0;

    /// Construct the fixed 600 Ω balanced line faces and their ordinary operating defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            detector_coeff_attack: 1.0,
            detector_coeff_release: 1.0,
            output_meter_coeff: 1.0,
            detector: 0.0,
            output_mean_square: 0.0,
            gain_reduction_db: 0.0,
            output_dbu: Self::READING_FLOOR_DBU,
            param_decls: [
                ParamDecl {
                    id: Self::PEAK_REDUCTION,
                    default: Self::PEAK_REDUCTION_DEFAULT,
                    min: 0.0,
                    max: 1.0,
                    smooth_ms: 10.0,
                },
                ParamDecl {
                    id: Self::GAIN,
                    default: Self::GAIN_DEFAULT,
                    min: 1.0,
                    max: 100.0,
                    smooth_ms: 5.0,
                },
                ParamDecl {
                    id: Self::LIMIT,
                    default: 0.0,
                    min: 0.0,
                    max: 1.0,
                    smooth_ms: 5.0,
                },
                ParamDecl {
                    id: Self::POWERED,
                    default: 1.0,
                    min: 0.0,
                    max: 1.0,
                    smooth_ms: 5.0,
                },
            ],
            readouts: [
                ReadoutDecl {
                    id: Self::GAIN_REDUCTION_DB,
                },
                ReadoutDecl {
                    id: Self::OUTPUT_DBU,
                },
            ],
            inputs: [InputZ::balanced(Ohms::new(Self::LINE_Z_OHMS)).into()],
            outputs: [OutputZ::balanced(Ohms::new(Self::LINE_Z_OHMS)).into()],
        }
    }

    fn coeff(rate: AnalogRate, ms: f64) -> f64 {
        let samples = ms * 1e-3 * rate.as_hz();
        if samples > 0.0 {
            1.0 - (-1.0 / samples).exp()
        } else {
            1.0
        }
    }

    /// The gain reduction in dB for the current detector level. This intentionally simple curve is
    /// a first-pass optical lesson, not a T4 cell emulation: Peak Reduction lowers the threshold from
    /// +20 to −20 dBu; Limit crossfades the ratio from 3:1 to 20:1.
    fn reduction_db(detector: f64, peak_reduction: f32, limit: f32) -> f64 {
        let level_dbu = f64::from(volts_to_dbu(Volts::new(detector as f32)));
        let threshold_dbu = 20.0 - 40.0 * f64::from(peak_reduction);
        let ratio =
            Self::COMPRESS_RATIO + (Self::LIMIT_RATIO - Self::COMPRESS_RATIO) * f64::from(limit);
        let slope = 1.0 - 1.0 / ratio;
        let over = level_dbu - threshold_dbu;
        let half_knee = Self::KNEE_DB / 2.0;
        let reduction = if over <= -half_knee {
            0.0
        } else if over >= half_knee {
            slope * over
        } else {
            let t = over + half_knee;
            slope * t * t / (2.0 * Self::KNEE_DB)
        };
        reduction.clamp(0.0, Self::MAX_REDUCTION_DB)
    }
}

impl Default for LumaLeveler {
    fn default() -> Self {
        Self::new()
    }
}

impl Node for LumaLeveler {
    fn inputs(&self) -> &[InputPort] {
        &self.inputs
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    fn params(&self) -> &[ParamDecl] {
        &self.param_decls
    }

    fn readouts(&self) -> &[ReadoutDecl] {
        &self.readouts
    }

    fn prepare(&mut self, rate: AnalogRate) {
        self.detector_coeff_attack = Self::coeff(rate, Self::ATTACK_MS);
        self.detector_coeff_release = Self::coeff(rate, Self::RELEASE_MS);
        self.output_meter_coeff = Self::coeff(rate, Self::OUTPUT_METER_MS);
    }

    fn process(&mut self, params: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
        let plus = inputs[0].voltage().as_slice();
        let minus = inputs[1].voltage().as_slice();
        let (out_plus, out_minus) = outputs.split_at_mut(1);
        let out_plus = out_plus[0].voltage_mut().as_mut_slice();
        let out_minus = out_minus[0].voltage_mut().as_mut_slice();
        let (attack, release, meter_coeff) = (
            self.detector_coeff_attack,
            self.detector_coeff_release,
            self.output_meter_coeff,
        );
        let (mut detector, mut output_mean_square) = (self.detector, self.output_mean_square);
        let mut gain_reduction_db = 0.0_f32;

        for (i, (((out_p, out_n), &p), &n)) in out_plus
            .iter_mut()
            .zip(out_minus.iter_mut())
            .zip(plus.iter())
            .zip(minus.iter())
            .enumerate()
        {
            // Difference first: a shared phantom pedestal or cable pickup cancels while both legs
            // remain linear. Compression never sees common-mode voltage.
            let differential = p - n;
            let magnitude = f64::from(differential.abs());
            let coeff = if magnitude > detector {
                attack
            } else {
                release
            };
            detector = flush_denormal(detector + coeff * (magnitude - detector));

            let peak_reduction =
                params.value_at_or(Self::PEAK_REDUCTION, i, Self::PEAK_REDUCTION_DEFAULT);
            let gain = params.value_at_or(Self::GAIN, i, Self::GAIN_DEFAULT);
            let limit = params.value_at_or(Self::LIMIT, i, 0.0);
            let powered = params.value_at_or(Self::POWERED, i, 1.0);
            let reduction = Self::reduction_db(detector, peak_reduction, limit);
            let compressed = f64::from(differential) * 10.0_f64.powf(-reduction / 20.0);
            // The rail belongs to one conductor. Clamp the differential to twice that rail before
            // splitting it, so both balanced legs stay equal and opposite even at output overload.
            let output_differential = (compressed * f64::from(gain)).clamp(
                -2.0 * f64::from(Self::OUTPUT_RAIL_PER_LEG_V),
                2.0 * f64::from(Self::OUTPUT_RAIL_PER_LEG_V),
            ) * f64::from(powered);
            *out_p = (0.5 * output_differential) as f32;
            *out_n = (-0.5 * output_differential) as f32;
            output_mean_square = flush_denormal(
                output_mean_square
                    + meter_coeff
                        * (output_differential * output_differential - output_mean_square),
            );
            gain_reduction_db = reduction as f32;
        }

        self.detector = detector;
        self.output_mean_square = output_mean_square;
        self.gain_reduction_db = gain_reduction_db;
        let rms = output_mean_square.sqrt() as f32;
        self.output_dbu = if rms > 0.0 {
            volts_to_dbu(Volts::new(rms)).max(Self::READING_FLOOR_DBU)
        } else {
            Self::READING_FLOOR_DBU
        };
    }

    fn read_readouts(&self, out: &mut [f32]) {
        out[0] = self.gain_reduction_db;
        out[1] = self.output_dbu;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::electrical::Ohms;
    use crate::graph::Graph;
    use crate::param::ParamQueue;
    use crate::signal::VoltageBuffer;
    use crate::test_util::{BalancedTestSource, process_voltage};
    use crate::{Volts, compile};
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    fn process(leveler: &mut LumaLeveler, plus: &[f32], minus: &[f32]) -> [VoltageBuffer; 2] {
        let input = [
            VoltageBuffer::from_volts(plus.to_vec(), rate()),
            VoltageBuffer::from_volts(minus.to_vec(), rate()),
        ];
        let mut output = [
            VoltageBuffer::zeros(plus.len(), rate()),
            VoltageBuffer::zeros(plus.len(), rate()),
        ];
        process_voltage(leveler, &input, &mut output);
        output
    }

    #[test]
    fn declares_fixed_balanced_line_faces_controls_and_readouts() {
        let leveler = LumaLeveler::new();
        assert_eq!(
            leveler.inputs(),
            &[InputPort::Analog(InputZ::balanced(Ohms::new(600.0)))]
        );
        assert_eq!(
            leveler.outputs(),
            &[OutputPort::Analog(OutputZ::balanced(Ohms::new(600.0)))]
        );
        assert_eq!(leveler.params().len(), 4);
        assert_eq!(leveler.readouts().len(), 2);
    }

    #[test]
    fn rejects_common_mode_and_preserves_the_differential_as_equal_opposite_legs() {
        // V+ = 3 V and V− = 1 V contain 2 V common mode plus a 2 V differential signal. At
        // unity gain/reduction the output differential is 3 − 1 = 2 V, split equally: +1 V and
        // −1 V. A second sample with V+ = V− = 4 V has zero differential and must be silent.
        let mut leveler = LumaLeveler::new();
        let out = process(&mut leveler, &[3.0, 4.0], &[1.0, 4.0]);
        assert_relative_eq!(out[0].get(0).get(), 1.0, epsilon = 1e-6);
        assert_relative_eq!(out[1].get(0).get(), -1.0, epsilon = 1e-6);
        assert_relative_eq!(out[0].get(1).get(), 0.0, epsilon = 1e-6);
        assert_relative_eq!(out[1].get(1).get(), 0.0, epsilon = 1e-6);
    }

    #[test]
    fn limit_reduces_more_than_compress_at_the_same_overdrive() {
        // At +22.22 dBu into the −20 dBu full-drive threshold, the 3:1 curve reduces less than
        // the 20:1 curve; both stay below the model's 40 dB ceiling.
        let compress = LumaLeveler::reduction_db(10.0, 1.0, 0.0);
        let limit = LumaLeveler::reduction_db(10.0, 1.0, 1.0);
        assert!(limit > compress);
        assert!(limit <= 40.0);
    }

    #[test]
    fn detector_reaches_sixty_three_percent_after_ten_ms() {
        // One 10 ms time constant at 384 kHz is 3,840 samples. A one-pole step reaches
        // 1 − e^(−1) = 0.63212 of its 1 V target; the uncompressed default has no effect here.
        let mut leveler = LumaLeveler::new();
        leveler.prepare(rate());
        let input = vec![1.0; 3_840];
        let silent = vec![0.0; input.len()];
        let _ = process(&mut leveler, &input, &silent);
        assert_relative_eq!(leveler.detector, 1.0 - (-1.0_f64).exp(), epsilon = 1e-3);
    }

    #[test]
    fn output_readout_is_measured_from_processed_voltage() {
        let mut leveler = LumaLeveler::new();
        leveler.prepare(rate());
        // A constant 1.2283 V differential signal is +4 dBu RMS. After several 65 ms meter
        // constants, its measured output level settles to +4 dBu — the value comes from volts,
        // not from the Gain knob.
        let input = vec![1.2283; 125_000];
        let silent = vec![0.0; input.len()];
        let _ = process(&mut leveler, &input, &silent);
        let mut readouts = [0.0; 2];
        leveler.read_readouts(&mut readouts);
        assert_relative_eq!(readouts[1], 4.0, epsilon = 0.05);
    }

    #[test]
    fn power_glides_the_balanced_output_to_silence() {
        let mut graph = Graph::new();
        let source = graph.add(BalancedTestSource::new(
            Volts::new(2.0),
            Volts::new(0.0),
            Ohms::new(1.0),
        ));
        let leveler = graph.add(LumaLeveler::new());
        graph.connect_ideal(source, 0, leveler, 0);
        graph.set_output(leveler, 0);

        let block = 64;
        let mut schedule = compile(graph, block, rate(), 0).expect("valid balanced leveler chain");
        let powered = schedule
            .param(leveler, LumaLeveler::POWERED)
            .expect("power param resolves");
        let mut out = VoltageBuffer::zeros(block, rate());
        schedule.process(&mut out);
        assert!(out.as_slice().iter().any(|&v| v.abs() > 0.1));

        // The 5 ms power glide is 1,920 samples at 384 kHz, or 30 blocks of 64. Run 40 blocks
        // after requesting power-off: the source remains live, so only the leveler's smoothed
        // output gate can make the tapped balanced hot leg settle at zero.
        let mut queue = ParamQueue::with_capacity(1);
        queue.set(powered, 0.0);
        for block_index in 0..40 {
            if block_index == 0 {
                schedule.process_with_params(&mut out, &mut queue);
            } else {
                schedule.process(&mut out);
            }
        }
        let peak = out
            .as_slice()
            .iter()
            .fold(0.0_f32, |max, &v| max.max(v.abs()));
        assert!(
            peak < 1e-4,
            "powered-off output must be silent, got {peak} V"
        );
    }
}
