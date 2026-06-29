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
/// recovered / external word clock) would live once multiple clock domains are modeled.
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

/// A MIDI/control-event port's face: the capacity (max events per block) of its lane.
///
/// The sparse-carrier peer of [`DigitalFace`]. An event lane is bounded, not block-length-sized,
/// so the face carries the bound `compile` pre-allocates; on a full block the hot path drops the
/// excess (see [`EventBuffer`](crate::EventBuffer)). The capacity is a per-port choice — a synth
/// voice needs only a few events per block, a busy MIDI router more.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventFace {
    capacity: usize,
}

impl EventFace {
    /// A generous default capacity for ports that don't have a specific bound in mind.
    pub const DEFAULT_CAPACITY: usize = 256;

    /// An event face whose lane holds up to `capacity` events per block.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self { capacity }
    }

    /// The lane's per-block event capacity.
    pub fn capacity(self) -> usize {
        self.capacity
    }
}

impl Default for EventFace {
    /// An event face at [`DEFAULT_CAPACITY`](Self::DEFAULT_CAPACITY).
    fn default() -> Self {
        Self::new(Self::DEFAULT_CAPACITY)
    }
}

/// A node input port: an analog input impedance, a digital-audio input face, or a control-event
/// input face.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputPort {
    /// Analog voltage input, with its input impedance.
    Analog(InputZ),
    /// Digital-audio input.
    Digital(DigitalFace),
    /// MIDI/control-event input.
    Events(EventFace),
}

/// A node output port: an analog output impedance, a digital-audio output face, or a control-event
/// output face.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputPort {
    /// Analog voltage output, with its output impedance.
    Analog(OutputZ),
    /// Digital-audio output.
    Digital(DigitalFace),
    /// MIDI/control-event output.
    Events(EventFace),
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

impl From<EventFace> for InputPort {
    fn from(f: EventFace) -> Self {
        InputPort::Events(f)
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

impl From<EventFace> for OutputPort {
    fn from(f: EventFace) -> Self {
        OutputPort::Events(f)
    }
}

impl InputPort {
    /// The carrier domain of this port.
    pub fn domain(self) -> Domain {
        match self {
            InputPort::Analog(_) => Domain::Analog,
            InputPort::Digital(_) => Domain::DigitalAudio,
            InputPort::Events(_) => Domain::Events,
        }
    }

    /// How many lanes (pool buffers) this port owns: conductors for analog (1 unbalanced,
    /// 2 balanced), channels for digital, and a single stream (1) for events.
    pub fn lane_count(self) -> usize {
        match self {
            InputPort::Analog(z) => z.conductors(),
            InputPort::Digital(f) => f.format().channels() as usize,
            InputPort::Events(_) => 1,
        }
    }

    /// The analog input impedance, if this is an analog port. Off the hot path; `compile` uses it
    /// to bake the divider solve for analog edges only.
    pub fn analog(self) -> Option<InputZ> {
        match self {
            InputPort::Analog(z) => Some(z),
            InputPort::Digital(_) | InputPort::Events(_) => None,
        }
    }

    /// The digital face, if this is a digital port. Off the hot path; `compile` reads its format
    /// to size the digital sample lanes (`block_len / M`).
    pub fn digital(self) -> Option<DigitalFace> {
        match self {
            InputPort::Digital(f) => Some(f),
            InputPort::Analog(_) | InputPort::Events(_) => None,
        }
    }

    /// The event face, if this is an event port. Off the hot path; `compile` reads its capacity
    /// to size the bounded event lane.
    pub fn events(self) -> Option<EventFace> {
        match self {
            InputPort::Events(f) => Some(f),
            InputPort::Analog(_) | InputPort::Digital(_) => None,
        }
    }
}

impl OutputPort {
    /// The carrier domain of this port.
    pub fn domain(self) -> Domain {
        match self {
            OutputPort::Analog(_) => Domain::Analog,
            OutputPort::Digital(_) => Domain::DigitalAudio,
            OutputPort::Events(_) => Domain::Events,
        }
    }

    /// How many lanes (pool buffers) this port owns: conductors for analog, channels for digital,
    /// and a single stream (1) for events.
    pub fn lane_count(self) -> usize {
        match self {
            OutputPort::Analog(z) => z.conductors(),
            OutputPort::Digital(f) => f.format().channels() as usize,
            OutputPort::Events(_) => 1,
        }
    }

    /// The analog output impedance, if this is an analog port. Off the hot path; `compile` uses it
    /// to bake the divider solve for analog edges only.
    pub fn analog(self) -> Option<OutputZ> {
        match self {
            OutputPort::Analog(z) => Some(z),
            OutputPort::Digital(_) | OutputPort::Events(_) => None,
        }
    }

    /// The digital face, if this is a digital port. Off the hot path; `compile` reads its format
    /// to size the digital sample lanes (`block_len / M`).
    pub fn digital(self) -> Option<DigitalFace> {
        match self {
            OutputPort::Digital(f) => Some(f),
            OutputPort::Analog(_) | OutputPort::Events(_) => None,
        }
    }

    /// The event face, if this is an event port. Off the hot path; `compile` reads its capacity
    /// to size the bounded event lane.
    pub fn events(self) -> Option<EventFace> {
        match self {
            OutputPort::Events(f) => Some(f),
            OutputPort::Analog(_) | OutputPort::Digital(_) => None,
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

    #[test]
    fn event_port_is_one_lane_in_the_events_domain() {
        let inp: InputPort = EventFace::new(64).into();
        let out: OutputPort = EventFace::default().into();
        assert_eq!(inp.domain(), Domain::Events);
        assert_eq!(out.domain(), Domain::Events);
        assert_eq!(inp.lane_count(), 1); // one event stream, not conductors/channels
        assert_eq!(out.lane_count(), 1);
        assert_eq!(inp.events().unwrap().capacity(), 64);
        assert_eq!(
            out.events().unwrap().capacity(),
            EventFace::DEFAULT_CAPACITY
        );
        // Not an analog or digital face.
        assert!(inp.analog().is_none() && inp.digital().is_none());
        assert!(out.analog().is_none() && out.digital().is_none());
    }
}
