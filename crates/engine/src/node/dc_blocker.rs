//! A DC-blocking high-pass — the AC-coupling series capacitor as a node.

use super::Node;
use crate::electrical::{Farads, InputZ, Ohms, OnePole, OutputZ};
use crate::param::Params;
use crate::port::{InputPort, OutputPort};
use crate::signal::{AnalogRate, Lane};

/// A DC blocker: a one-pole **high-pass** that passes audio and rejects DC, the dual of the
/// cable's [`OnePole`] low-pass.
///
/// Models **AC coupling** — a series capacitor `c` into a bias resistor `r` to ground, the
/// pairing real device inputs and outputs use to strip the DC offset a stage sits at while
/// passing the signal. A capacitor passes *change* and blocks the steady, so the C·R pair is a
/// high-pass: gain → 0 at DC (0 Hz), → 1 in the passband, with the −3 dB corner at
/// `f_c = 1/(2π·r·c)`, set low (a few hertz) so all of audio rides through untouched.
///
/// **Why it removes DC.** A high-pass is its low-pass complement: `out = x − lowpass(x)`. The
/// inner low-pass tracks the slow-moving part of the signal — left alone it converges to the DC
/// level — and subtracting that off leaves only the AC. So the node reuses an [`OnePole`] as a
/// *DC tracker* (its per-sample `step` seam) rather than reimplementing the recurrence:
/// one pole, two filters. In transfer-function terms it places a **zero at DC** (exact
/// blocking) and the same matched pole the low-pass has.
///
/// The corner is computed from this node's own `r` and `c` at [`prepare`](Node::prepare). The
/// bias resistor is high-Z (≫ any source impedance), so it dominates the RC and the upstream
/// source's resistance is a negligible contributor to the corner — leaving the resistive
/// loading divider, as always, to the connection. The node presents `InputZ = r` (the bias
/// resistor is what a source sees in the passband, where the cap is ~a short).
///
/// One input; one output.
pub struct DcBlocker {
    /// The AC-coupling series capacitor.
    c: Farads,
    /// The bias resistor to ground: sets the corner with `c`, and is the input impedance.
    r: Ohms,
    /// The DC tracker (an [`OnePole`] low-pass), baked from `r`, `c` and the rate at
    /// [`prepare`](Node::prepare). `None` until prepared — an unprepared blocker passes through.
    tracker: Option<OnePole>,
    inputs: [InputPort; 1],
    outputs: [OutputPort; 1],
}

impl DcBlocker {
    /// A DC blocker with AC-coupling capacitor `c` and bias resistor `r` (also its input
    /// impedance), driving from output impedance `z_out`. The high-pass corner is
    /// `f_c = 1/(2π·r·c)` — see [`corner_hz`](Self::corner_hz).
    #[must_use]
    pub fn new(c: Farads, r: Ohms, z_out: Ohms) -> Self {
        Self {
            c,
            r,
            tracker: None,
            inputs: [InputZ::new(r).into()],
            outputs: [OutputZ::new(z_out).into()],
        }
    }

    /// The −3 dB high-pass corner in hertz, `f_c = 1/(2π·r·c)`. Below it the response rolls
    /// off toward 0 (DC fully blocked); above it the gain is ~unity. Off the hot path.
    #[must_use]
    pub fn corner_hz(&self) -> f64 {
        let rc = f64::from(self.r.get()) * f64::from(self.c.get());
        1.0 / (core::f64::consts::TAU * rc)
    }
}

impl Node for DcBlocker {
    fn inputs(&self) -> &[InputPort] {
        &self.inputs
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    fn prepare(&mut self, rate: AnalogRate) {
        // Bake the DC tracker's pole here (an `exp`), off the hot path; `process` only steps it.
        self.tracker = Some(OnePole::new(self.r, self.c, rate));
    }

    fn per_conductor(&self) -> bool {
        // AC coupling is a per-leg series cap: on a balanced pair it's one blocker on each
        // conductor. So the compiler may lift this node across a balanced connection — each leg
        // gets its own identical high-pass (Story 1.5 detour).
        true
    }

    fn replicate(&self) -> Box<dyn Node> {
        // A fresh, unprepared blocker with the same R/C/Zout; `compile` prepares each lane, which
        // bakes its own tracker (zeroed state).
        let z_out = self.outputs[0]
            .analog()
            .expect("DC blocker output is analog")
            .z_out();
        Box::new(DcBlocker::new(self.c, self.r, z_out))
    }

    fn process(&mut self, _params: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
        let src = inputs[0].voltage().as_slice();
        let out = outputs[0].voltage_mut().as_mut_slice();
        match &mut self.tracker {
            Some(tracker) => {
                for (o, &v) in out.iter_mut().zip(src) {
                    // High-pass = input − its tracked DC/low content; the DC settles into the
                    // tracker and is subtracted away, leaving the AC.
                    let x = f64::from(v);
                    *o = (x - tracker.step(x)) as f32;
                }
            }
            None => {
                // Unprepared (no rate yet): pass through. `compile` always prepares before
                // `process`, so a compiled schedule never hits this arm.
                for (o, &v) in out.iter_mut().zip(src) {
                    *o = v;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::{VoltageBuffer, Volts};
    use crate::test_util::{measure_gain, process_voltage, rms};
    use approx::assert_relative_eq;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    /// A blocker with a ~1 kHz corner: r = 10 kΩ, c = 15.915 nF ⇒ f_c = 1/(2π·1e4·15.915e-9) =
    /// 1000 Hz. (A real DC blocker corners near 5 Hz; 1 kHz keeps the test buffers short while
    /// sitting far below Nyquist so the passband stays flat.)
    fn blocker() -> DcBlocker {
        DcBlocker::new(
            Farads::new(15.915e-9),
            Ohms::new(10_000.0),
            Ohms::new(150.0),
        )
    }

    /// Run the (prepared) blocker over a buffer once via the two-pool node interface, in place,
    /// so [`measure_gain`] can drive a sine through it.
    fn through(blk: &mut DcBlocker, buf: &mut VoltageBuffer) {
        let input = [buf.clone()];
        let mut out = [VoltageBuffer::zeros(buf.len(), buf.rate())];
        process_voltage(blk, &input, &mut out);
        buf.as_mut_slice().copy_from_slice(out[0].as_slice());
    }

    #[test]
    fn declares_faces() {
        let b = blocker();
        assert_eq!(
            b.inputs(),
            &[InputPort::Analog(InputZ::new(Ohms::new(10_000.0)))]
        );
        assert_eq!(
            b.outputs(),
            &[OutputPort::Analog(OutputZ::new(Ohms::new(150.0)))]
        );
    }

    #[test]
    fn corner_matches_the_rc() {
        // f_c = 1/(2π·10000·15.915e-9) = 1000.0 Hz.
        assert_relative_eq!(blocker().corner_hz(), 1000.0, max_relative = 1e-3);
    }

    #[test]
    fn blocks_dc() {
        // Constant 1 V in. The tracker converges to 1 V (residual e^(−n·dt/RC)), so the
        // high-pass output decays to 0. After thousands of samples the residual underflows to
        // a denormal and is flushed — the tail is silence. DC in ⇒ no DC out.
        let mut b = blocker();
        b.prepare(rate());
        let mut buf = VoltageBuffer::zeros(4_000, rate());
        buf.fill(Volts::new(1.0));
        let input = [buf];
        let mut out = [VoltageBuffer::zeros(4_000, rate())];
        process_voltage(&mut b, &input, &mut out);
        // Well past settling (τ = RC ≈ 61 samples), the output is ≈ 0.
        let tail = &out[0].as_slice()[2_000..];
        assert!(
            tail.iter().all(|&v| v.abs() < 1e-4),
            "DC should be blocked: max tail = {}",
            tail.iter().fold(0.0_f32, |m, &v| m.max(v.abs()))
        );
    }

    #[test]
    fn passes_audio_a_decade_above_the_corner() {
        // 10 kHz is a decade above the 1 kHz corner and far below Nyquist → ~unity gain.
        let mut b = blocker();
        b.prepare(rate());
        let g = measure_gain(10_000.0, rate(), |buf| through(&mut b, buf));
        assert!(
            g > 0.98,
            "passband should be ~unity well above f_c, got {g}"
        );
    }

    #[test]
    fn is_minus_3_db_at_the_corner() {
        // A one-pole high-pass is −3 dB (gain 0.707) at its corner, the mirror of the low-pass.
        let mut b = blocker();
        b.prepare(rate());
        let g = measure_gain(1_000.0, rate(), |buf| through(&mut b, buf));
        assert_relative_eq!(g, 0.707_106_77, epsilon = 1.5e-2);
    }

    #[test]
    fn rolls_off_below_the_corner() {
        // Two octaves below f_c (250 Hz) the high-pass is well into its rolloff:
        // |H| = 1/√(1+(f_c/f)²) = 1/√(1+16) ≈ 0.243.
        let mut b = blocker();
        b.prepare(rate());
        let g = measure_gain(250.0, rate(), |buf| through(&mut b, buf));
        assert!(g < 0.30, "should be well into rolloff below f_c, got {g}");
    }

    #[test]
    fn passes_audio_unchanged_but_centered() {
        // A 10 kHz sine on a 2 V DC pedestal: the AC rides through ~unity, the pedestal is
        // removed, so the output RMS is the sine's (amp/√2) with no DC contribution.
        let mut b = blocker();
        b.prepare(rate());
        let len = 40_000; // ≫ settling; whole cycles of 10 kHz at 384 kHz
        let mut buf = VoltageBuffer::zeros(len, rate());
        let dt = rate().seconds_per_sample();
        let omega = core::f64::consts::TAU * 10_000.0;
        for (n, s) in buf.as_mut_slice().iter_mut().enumerate() {
            *s = (2.0 + (omega * (n as f64 * dt)).sin()) as f32; // 2 V DC + 1 V sine
        }
        let input = [buf];
        let mut out = [VoltageBuffer::zeros(len, rate())];
        process_voltage(&mut b, &input, &mut out);

        let tail = &out[0].as_slice()[len / 2..];
        // DC removed: the mean of the steady tail is ≈ 0 (the 2 V pedestal is gone).
        let mean: f64 = tail.iter().map(|&v| f64::from(v)).sum::<f64>() / tail.len() as f64;
        assert!(mean.abs() < 1e-2, "DC offset should be gone, mean = {mean}");
        // AC preserved: RMS ≈ amp/√2 = 0.7071 (the sine survives the passband).
        assert_relative_eq!(rms(tail), 0.707_106_77, max_relative = 2e-2);
    }

    #[test]
    fn unprepared_passes_through() {
        // No prepare() ⇒ no rate ⇒ the safe default is identity (DC not yet removed).
        let mut b = blocker();
        let mut buf = VoltageBuffer::zeros(8, rate());
        buf.fill(Volts::new(1.0));
        let input = [buf];
        let mut out = [VoltageBuffer::zeros(8, rate())];
        process_voltage(&mut b, &input, &mut out);
        assert!(out[0].as_slice().iter().all(|&v| v == 1.0));
    }
}
