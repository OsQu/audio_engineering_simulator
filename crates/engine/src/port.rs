//! Ports: a node's domain-tagged connection points.
//!
//! A [`Node`](crate::Node) declares its faces as [`InputPort`]s and [`OutputPort`]s. Each is a
//! per-direction enum **wrapping the existing electrical face** ([`InputZ`] / [`OutputZ`]) for the
//! analog domain, or a [`DigitalFace`] for digital audio. Keeping `InputZ` / `OutputZ` unchanged
//! means analog nodes' impedance API doesn't churn, and source-vs-load impedance stays
//! type-distinct; the per-direction asymmetry is honest (analog `z` differs by direction, a digital
//! format does not). `compile` reads [`domain`](InputPort::domain) and
//! [`lane_count`](InputPort::lane_count) off every port uniformly — the latter unifying pool
//! allocation (conductors for analog, channels for digital) while the balanced-lift semantics stay
//! an analog-only property.

use crate::electrical::{InputZ, OutputZ};
use crate::signal::{BitDepth, Domain, SampleRate};

/// The stream format of a digital-audio port: sample rate, bit depth, and channel count.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioFormat {
    rate: SampleRate,
    bits: BitDepth,
    channels: u16,
}

impl AudioFormat {
    /// A format of `channels` channels at `rate` / `bits`.
    ///
    /// # Panics
    /// Panics if `channels == 0`. A construction-time check (faces are fixed up front).
    #[must_use]
    pub fn new(rate: SampleRate, bits: BitDepth, channels: u16) -> Self {
        assert!(channels >= 1, "AudioFormat needs at least one channel");
        Self {
            rate,
            bits,
            channels,
        }
    }

    /// The sample rate.
    pub fn rate(self) -> SampleRate {
        self.rate
    }

    /// The bit depth.
    pub fn bits(self) -> BitDepth {
        self.bits
    }

    /// The channel count.
    pub fn channels(self) -> u16 {
        self.channels
    }
}

/// A digital-audio port's face: its stream format.
///
/// A thin wrapper over [`AudioFormat`] now; it is where a port's **clock role** (internal /
/// recovered / external word clock) will live when the emergent clock model lands (Epic 5).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DigitalFace {
    format: AudioFormat,
}

impl DigitalFace {
    /// A digital face carrying `format`.
    #[must_use]
    pub fn new(format: AudioFormat) -> Self {
        Self { format }
    }

    /// The stream format.
    pub fn format(self) -> AudioFormat {
        self.format
    }
}

/// A node input port: an analog input impedance, or a digital-audio input face.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputPort {
    /// Analog voltage input, with its input impedance.
    Analog(InputZ),
    /// Digital-audio input.
    Digital(DigitalFace),
}

/// A node output port: an analog output impedance, or a digital-audio output face.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputPort {
    /// Analog voltage output, with its output impedance.
    Analog(OutputZ),
    /// Digital-audio output.
    Digital(DigitalFace),
}

impl From<InputZ> for InputPort {
    fn from(z: InputZ) -> Self {
        InputPort::Analog(z)
    }
}

impl From<DigitalFace> for InputPort {
    fn from(f: DigitalFace) -> Self {
        InputPort::Digital(f)
    }
}

impl From<OutputZ> for OutputPort {
    fn from(z: OutputZ) -> Self {
        OutputPort::Analog(z)
    }
}

impl From<DigitalFace> for OutputPort {
    fn from(f: DigitalFace) -> Self {
        OutputPort::Digital(f)
    }
}

impl InputPort {
    /// The carrier domain of this port.
    pub fn domain(self) -> Domain {
        match self {
            InputPort::Analog(_) => Domain::Analog,
            InputPort::Digital(_) => Domain::DigitalAudio,
        }
    }

    /// How many lanes (pool buffers) this port owns: conductors for analog (1 unbalanced,
    /// 2 balanced), channels for digital.
    pub fn lane_count(self) -> usize {
        match self {
            InputPort::Analog(z) => z.conductors(),
            InputPort::Digital(f) => f.format().channels() as usize,
        }
    }

    /// The analog input impedance, if this is an analog port. Off the hot path; `compile` uses it
    /// to bake the divider solve for analog edges only.
    pub fn analog(self) -> Option<InputZ> {
        match self {
            InputPort::Analog(z) => Some(z),
            InputPort::Digital(_) => None,
        }
    }

    /// The digital face, if this is a digital port. Off the hot path; `compile` reads its format
    /// to size the digital sample lanes (`block_len / M`).
    pub fn digital(self) -> Option<DigitalFace> {
        match self {
            InputPort::Digital(f) => Some(f),
            InputPort::Analog(_) => None,
        }
    }
}

impl OutputPort {
    /// The carrier domain of this port.
    pub fn domain(self) -> Domain {
        match self {
            OutputPort::Analog(_) => Domain::Analog,
            OutputPort::Digital(_) => Domain::DigitalAudio,
        }
    }

    /// How many lanes (pool buffers) this port owns: conductors for analog, channels for digital.
    pub fn lane_count(self) -> usize {
        match self {
            OutputPort::Analog(z) => z.conductors(),
            OutputPort::Digital(f) => f.format().channels() as usize,
        }
    }

    /// The analog output impedance, if this is an analog port. Off the hot path; `compile` uses it
    /// to bake the divider solve for analog edges only.
    pub fn analog(self) -> Option<OutputZ> {
        match self {
            OutputPort::Analog(z) => Some(z),
            OutputPort::Digital(_) => None,
        }
    }

    /// The digital face, if this is a digital port. Off the hot path; `compile` reads its format
    /// to size the digital sample lanes (`block_len / M`).
    pub fn digital(self) -> Option<DigitalFace> {
        match self {
            OutputPort::Digital(f) => Some(f),
            OutputPort::Analog(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::electrical::Ohms;

    fn digital_face(channels: u16) -> DigitalFace {
        DigitalFace::new(AudioFormat::new(
            SampleRate::new(48_000.0),
            BitDepth::new(24),
            channels,
        ))
    }

    #[test]
    fn analog_input_from_input_z() {
        let port: InputPort = InputZ::new(Ohms::new(10_000.0)).into();
        assert_eq!(port.domain(), Domain::Analog);
        assert_eq!(port.lane_count(), 1); // unbalanced
        assert_eq!(port.analog().unwrap().z_in(), Ohms::new(10_000.0));
    }

    #[test]
    fn balanced_analog_input_owns_two_lanes() {
        let port: InputPort = InputZ::balanced(Ohms::new(20_000.0)).into();
        assert_eq!(port.lane_count(), 2);
    }

    #[test]
    fn analog_output_from_output_z() {
        let port: OutputPort = OutputZ::new(Ohms::new(150.0)).into();
        assert_eq!(port.domain(), Domain::Analog);
        assert_eq!(port.lane_count(), 1);
        assert_eq!(port.analog().unwrap().z_out(), Ohms::new(150.0));
    }

    #[test]
    fn digital_port_reports_channels_and_no_analog_face() {
        let inp: InputPort = digital_face(8).into();
        let out: OutputPort = digital_face(2).into();
        assert_eq!(inp.domain(), Domain::DigitalAudio);
        assert_eq!(inp.lane_count(), 8); // ADAT-like: channels, not conductors
        assert_eq!(out.lane_count(), 2); // SPDIF-like
        assert!(inp.analog().is_none());
        assert!(out.analog().is_none());
    }

    #[test]
    #[should_panic(expected = "at least one channel")]
    fn rejects_zero_channels() {
        let _ = AudioFormat::new(SampleRate::new(48_000.0), BitDepth::new(24), 0);
    }
}
