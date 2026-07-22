//! A powered monitor: a voltage→acoustic transducer — the output-side mirror of the microphone.

use super::Node;
use crate::electrical::InputZ;
use crate::param::{ParamDecl, ParamId, Params};
use crate::port::{AcousticFace, InputPort, OutputPort};
use crate::signal::Lane;

/// A powered monitor (or guitar cab / PA speaker) — a **voltage→acoustic** transducer that radiates
/// the drive voltage it's fed as sound pressure: `pressure = drive_volts · sensitivity · volume`.
///
/// This is the output-side bookend to the [`DynamicMic`](super::DynamicMic): where a mic transduces
/// capsule pressure to a balanced voltage (`v = pressure · sensitivity`, V/Pa), the monitor does the
/// exact reverse (`pressure = v · sensitivity`, **Pa/V**), pointwise at the same analog rate — no
/// anti-alias FIR, no resample (converters other than the AD/DA never resample; §5.6). A source you
/// can now *mic* on the way out: patch its acoustic output into a mic capsule and the whole
/// gain-before-feedback / "ring out the monitors" story follows from the physics + a delayed air edge.
///
/// **Distinct from the [`Speaker`](super::Speaker) terminus.** `Speaker` is the §5.5 voltage-capture
/// tap — it stays in volts (the harness samples its output as "what we hear"). This node instead
/// *leaves* the voltage domain: its output is a [`Pressure`](crate::Lane::Pressure) lane radiating
/// into the air, the seam a mic picks back up.
///
/// - **Sensitivity** (Pa/V) is a fixed transducer property set at [`new`](Self::new), not a knob —
///   the calibration anchor is **1 Pa/V** (1 V drive ⇒ 1 Pa peak ≈ 94 dB SPL at unity volume).
/// - **Volume** is the [`VOLUME`](Self::VOLUME) control param: a smoothed **attenuator from unity**
///   (0 dB) down to silence — a monitor's level pot, exactly like the interface monitor amp. It does
///   not boost; loop gain is pushed upstream (the preamp), which is where the feedback lesson lives.
///
/// Air stays ideal (no distance/room/propagation), so radiation is a pointwise scale — no state, no
/// analog rate needed. One analog input; one mono acoustic output.
pub struct PoweredMonitor {
    /// Transducer sensitivity, pascals per volt (`pressure = drive_volts · sensitivity`).
    sensitivity: f32,
    /// The [`VOLUME`](Self::VOLUME) default (the fallback when run outside a schedule).
    default_volume: f32,
    /// The declared control param: [`VOLUME`](Self::VOLUME).
    params: [ParamDecl; 1],
    inputs: [InputPort; 1],
    outputs: [OutputPort; 1],
}

impl PoweredMonitor {
    /// Volume: a smoothed linear level control (a multiplier in `[0, 1]` — an attenuator from unity).
    /// The host drives it with `(node, PoweredMonitor::VOLUME)`; uncontrolled it holds unity.
    pub const VOLUME: ParamId = ParamId(0);

    /// De-zipper glide for the volume knob — short enough to feel instant (matching the gain stages).
    const VOLUME_SMOOTH_MS: f32 = 5.0;

    /// A powered monitor of `sensitivity` pascals per volt (~1.0 = 1 Pa/V, the calibration anchor:
    /// 1 V ⇒ 1 Pa ≈ 94 dB SPL at unity volume), presenting the bridging input impedance `z_in`.
    ///
    /// Like the [`Speaker`](super::Speaker) it is a line-level *bridging* input — it doesn't load its
    /// source (we model neither speaker current draw nor amplifier power, §5.5), so `z_in` is high and
    /// never enters a divider solve in anger. Volume defaults to unity (0 dB).
    #[must_use]
    pub fn new(sensitivity: f32, z_in: InputZ) -> Self {
        Self {
            sensitivity,
            default_volume: 1.0,
            params: [ParamDecl {
                id: Self::VOLUME,
                default: 1.0,
                min: 0.0,
                max: 1.0,
                smooth_ms: Self::VOLUME_SMOOTH_MS,
            }],
            inputs: [z_in.into()],
            outputs: [AcousticFace::new().into()],
        }
    }
}

impl Node for PoweredMonitor {
    fn inputs(&self) -> &[InputPort] {
        &self.inputs
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    fn params(&self) -> &[ParamDecl] {
        &self.params
    }

    fn process(&mut self, params: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
        // Radiation is the reverse of the mic's transduction: pressure = drive_volts · sensitivity,
        // scaled by the smoothed volume (read per sample so a level change de-zippers across the
        // block; `default_volume` is used only when run without a schedule). Ideal air, so it's a
        // pointwise scale — no state, no rate.
        let sensitivity = self.sensitivity;
        let src = inputs[0].voltage().as_slice();
        let out = outputs[0].pressure_mut().as_mut_slice();
        for (i, (o, &v)) in out.iter_mut().zip(src).enumerate() {
            let volume = params.value_at_or(Self::VOLUME, i, self.default_volume);
            *o = v * sensitivity * volume;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::electrical::Ohms;
    use crate::level::pascals_to_spl;
    use crate::param::Smoother;
    use crate::signal::{AnalogRate, Domain, Pascals, PressureBuffer, VoltageBuffer, Volts};
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    fn monitor(sensitivity: f32) -> PoweredMonitor {
        PoweredMonitor::new(sensitivity, InputZ::new(Ohms::new(10_000.0)))
    }

    /// A monitor of `sensitivity` (Pa/V) fed a constant drive `v` for `n` samples at the given
    /// `volume` (unity if `None`, i.e. run without a schedule); returns the radiated pressure block.
    fn run_monitor(sensitivity: f32, v: Volts, n: usize, volume: Option<f32>) -> PressureBuffer {
        let mut m = monitor(sensitivity);
        let mut in_lane = [Lane::Voltage(VoltageBuffer::zeros(n, rate()))];
        in_lane[0].voltage_mut().fill(v);
        let mut out = [Lane::Pressure(PressureBuffer::zeros(n, rate()))];
        match volume {
            None => m.process(&Params::EMPTY, &in_lane, &mut out),
            Some(vol) => {
                // A settled smoother sits at its value (step 0), so it reads `vol` at every sample.
                let smoothers = [Smoother::new(vol, 0.0, 1.0, 1.0)];
                m.process(&Params::new(&smoothers), &in_lane, &mut out);
            }
        }
        match out.into_iter().next().unwrap() {
            Lane::Pressure(b) => b,
            _ => unreachable!(),
        }
    }

    #[test]
    fn declares_an_analog_input_and_a_mono_acoustic_output() {
        let m = monitor(1.0);
        assert_eq!(m.inputs().len(), 1);
        assert_eq!(m.inputs()[0].domain(), Domain::Analog);
        assert_eq!(m.outputs().len(), 1);
        assert_eq!(m.outputs()[0].domain(), Domain::Acoustic);
        assert_eq!(m.outputs()[0].lane_count(), 1); // mono — air is ideal
        // One control param: VOLUME, unity by default.
        let ps = m.params();
        assert_eq!(ps.len(), 1);
        assert_eq!(ps[0].id, PoweredMonitor::VOLUME);
        assert_relative_eq!(ps[0].default, 1.0);
        assert_relative_eq!(ps[0].max, 1.0); // an attenuator: never boosts past unity
    }

    #[test]
    fn radiates_drive_volts_times_sensitivity_at_the_calibration_anchor() {
        // The 1 Pa/V anchor at unity volume: 1 V drive · 1 Pa/V · 1 = 1 Pa peak, which is 94 dB SPL
        // exactly (the calibration anchor 1 Pa = 93.98 dB SPL). No schedule ⇒ volume falls back to
        // its unity default.
        let buf = run_monitor(1.0, Volts::new(1.0), 8, None);
        for k in 0..8 {
            assert_relative_eq!(buf.get(k).get(), 1.0, epsilon = 1e-9);
        }
        assert_relative_eq!(
            pascals_to_spl(Pascals::new(buf.get(0).get())),
            93.979_4,
            epsilon = 1e-3
        );
    }

    #[test]
    fn sensitivity_scales_the_radiated_pressure() {
        // A less efficient transducer radiates less for the same drive: 2 V · 0.5 Pa/V = 1 Pa;
        // and the scale is linear — negative drive radiates negative pressure (a rarefaction).
        let buf = run_monitor(0.5, Volts::new(2.0), 4, None);
        assert_relative_eq!(buf.get(0).get(), 1.0, epsilon = 1e-9);
        let buf = run_monitor(0.5, Volts::new(-2.0), 4, None);
        assert_relative_eq!(buf.get(0).get(), -1.0, epsilon = 1e-9);
    }

    #[test]
    fn volume_attenuates_from_unity() {
        // Volume is an attenuator: −6 dB ≈ ×0.501187 halves the pressure (−6.02 dB). 1 V · 1 Pa/V ·
        // 0.501187 = 0.501187 Pa, i.e. 6.02 dB below the 1 Pa (94 dB SPL) unity reading.
        let minus_6db = 10.0_f32.powf(-6.0 / 20.0); // 0.501187
        let buf = run_monitor(1.0, Volts::new(1.0), 4, Some(minus_6db));
        assert_relative_eq!(buf.get(0).get(), 0.501_187, epsilon = 1e-5);
        // Volume 0 is silence.
        let buf = run_monitor(1.0, Volts::new(1.0), 4, Some(0.0));
        assert!(buf.as_slice().iter().all(|&p| p == 0.0));
    }
}
