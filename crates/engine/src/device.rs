//! Devices: black boxes that transform voltage between their electrical terminals.
//!
//! A device presents **real electrical faces** (PROJECT_PLAN §5.3): each input has an
//! input impedance ([`InputZ`]), each output an output impedance ([`OutputZ`] — the `Zout`
//! of a [`Thevenin`](crate::Thevenin) source). We model the *observable I/O*, not the
//! circuitry inside — the transform from input voltages to output (open-circuit) voltages.
//!
//! The fixed electrical faces are declared up front ([`Device::inputs`] / [`Device::outputs`]);
//! the dynamic per-block signal flows through [`Device::process`]. The voltage *between*
//! devices — the loading divider and cable rolloff — is owned by the connection, not the
//! device, and applied by the schedule, so a device's output is always its **open-circuit**
//! `v_src` (what it would produce into an infinite load).

mod gain;
mod source;
mod sum;

pub use gain::GainStage;
pub use source::TestSource;
pub use sum::PassiveSum;

use crate::electrical::{InputZ, OutputZ};
use crate::signal::VoltageBuffer;

/// A black-box device: fixed electrical faces plus a per-block voltage transform.
///
/// # Hot-path contract
/// [`process`](Self::process) is on the audio path: it must **not allocate, panic, or
/// block**. All fallible setup (sizing, validation) happens earlier, at graph construction
/// and compile. The schedule owns every buffer and hands `process` the device's own input
/// and output blocks as already-sized slices — the device only reads inputs and writes
/// outputs.
///
/// # Buffers
/// `inputs` and `outputs` are the device's ports in declaration order: `inputs[i]` carries
/// the (already loaded-and-filtered) voltage arriving at input port `i`; the device writes
/// the open-circuit voltage of output port `j` into `outputs[j]`. Every block is the same
/// length, fixed at compile. An input port with nothing connected reads silence.
pub trait Device {
    /// The input impedance of each input port, in declaration order. Its length is the
    /// device's input-port count and must stay constant for the device's lifetime.
    fn inputs(&self) -> &[InputZ];

    /// The output impedance ([`OutputZ`]) of each output port, in declaration order. Its
    /// length is the device's output-port count and must stay constant for the device's
    /// lifetime.
    fn outputs(&self) -> &[OutputZ];

    /// Transform a block: read `inputs`, write each output port's **open-circuit** voltage
    /// into `outputs`. Hot path — no allocation, no panic. `inputs.len()` equals
    /// [`inputs`](Self::inputs)`.len()` and likewise for `outputs`.
    fn process(&mut self, inputs: &[VoltageBuffer], outputs: &mut [VoltageBuffer]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::electrical::Ohms;
    use crate::signal::{AnalogRate, Volts};

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    /// A minimal device exercising the trait shape: one input, one output, doubles the
    /// signal. Confirms the declared port counts line up with the slices `process` receives.
    struct Doubler {
        inputs: [InputZ; 1],
        outputs: [OutputZ; 1],
    }

    impl Device for Doubler {
        fn inputs(&self) -> &[InputZ] {
            &self.inputs
        }

        fn outputs(&self) -> &[OutputZ] {
            &self.outputs
        }

        fn process(&mut self, inputs: &[VoltageBuffer], outputs: &mut [VoltageBuffer]) {
            for (out, &v) in outputs[0].as_mut_slice().iter_mut().zip(inputs[0].as_slice()) {
                *out = v * 2.0;
            }
        }
    }

    #[test]
    fn port_declarations_match_the_process_slices() {
        let mut dev = Doubler {
            inputs: [InputZ::new(Ohms::new(10_000.0))],
            outputs: [OutputZ::new(Ohms::new(150.0))],
        };
        assert_eq!(dev.inputs().len(), 1);
        assert_eq!(dev.outputs().len(), 1);

        let mut input = [VoltageBuffer::zeros(4, rate())];
        input[0].fill(Volts::new(0.5));
        let mut output = [VoltageBuffer::zeros(4, rate())];
        dev.process(&input, &mut output);

        assert!(output[0].as_slice().iter().all(|&v| (v - 1.0).abs() < 1e-6));
    }
}
