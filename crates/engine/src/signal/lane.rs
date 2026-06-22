//! A carrier lane: one buffer in the schedule pool, tagged by its signal domain.

use super::{SampleBuffer, VoltageBuffer};

/// Which carrier a port — and the lane buffering it — speaks.
///
/// An **open set**: analog voltage and digital audio exist now; MIDI/control events (Story 1.7)
/// and networked audio (Epic 5) extend it. A port declares its domain; an edge may only connect
/// two ports of the **same** domain (`compile` rejects a cross-domain edge). Converters bridge
/// domains *inside* a node, never on an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Domain {
    /// Analog voltage — oversampled, in volts, at the one [`AnalogRate`](crate::AnalogRate).
    Analog,
    /// Digital audio — linear normalized samples at a converter's
    /// [`SampleRate`](crate::SampleRate) and [`ClockDomainId`](crate::ClockDomainId).
    DigitalAudio,
}

/// One lane in the schedule pool: a single conductor's (analog) or channel's (digital) buffer,
/// in whichever carrier the port speaks.
///
/// An **open** enum so a new carrier is an additive variant, not a re-plumb (the lane
/// representation must also admit non-dense carriers like MIDI events later, so it is never
/// assumed to be a dense `f32` block). `Node::process` takes `&[Lane]` and reads each lane through
/// the typed accessors below; because `compile` validated every edge's domain, a node only ever
/// receives the variants its ports declared — so the mismatched accessor arm is dead
/// (`unreachable!`), and the hot path stays panic-free in practice.
#[derive(Debug, Clone, PartialEq)]
pub enum Lane {
    /// Analog voltage.
    Voltage(VoltageBuffer),
    /// Digital audio.
    Sample(SampleBuffer),
}

impl Lane {
    /// The carrier domain of this lane.
    pub fn domain(&self) -> Domain {
        match self {
            Lane::Voltage(_) => Domain::Analog,
            Lane::Sample(_) => Domain::DigitalAudio,
        }
    }

    /// Number of samples in the lane's block.
    pub fn len(&self) -> usize {
        match self {
            Lane::Voltage(b) => b.len(),
            Lane::Sample(b) => b.len(),
        }
    }

    /// Whether the lane's block has no samples.
    pub fn is_empty(&self) -> bool {
        match self {
            Lane::Voltage(b) => b.is_empty(),
            Lane::Sample(b) => b.is_empty(),
        }
    }

    /// Read the lane as analog voltage.
    ///
    /// # Panics
    /// Panics if this is not a [`Voltage`](Lane::Voltage) lane. Dead in practice: `compile`
    /// validates that every wired port's domain matches, so a node is only ever handed the
    /// variant its face declared. Hot path — no allocation.
    #[inline]
    pub fn voltage(&self) -> &VoltageBuffer {
        match self {
            Lane::Voltage(b) => b,
            _ => unreachable!("lane is not Voltage — compile validates port domains"),
        }
    }

    /// Write the lane as analog voltage. See [`voltage`](Self::voltage) for the panic contract.
    #[inline]
    pub fn voltage_mut(&mut self) -> &mut VoltageBuffer {
        match self {
            Lane::Voltage(b) => b,
            _ => unreachable!("lane is not Voltage — compile validates port domains"),
        }
    }

    /// Read the lane as digital audio. See [`voltage`](Self::voltage) for the panic contract.
    #[inline]
    pub fn sample(&self) -> &SampleBuffer {
        match self {
            Lane::Sample(b) => b,
            _ => unreachable!("lane is not Sample — compile validates port domains"),
        }
    }

    /// Write the lane as digital audio. See [`voltage`](Self::voltage) for the panic contract.
    #[inline]
    pub fn sample_mut(&mut self) -> &mut SampleBuffer {
        match self {
            Lane::Sample(b) => b,
            _ => unreachable!("lane is not Sample — compile validates port domains"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::{AnalogRate, BitDepth, ClockDomainId, SampleRate};

    fn voltage_lane() -> Lane {
        Lane::Voltage(VoltageBuffer::zeros(4, AnalogRate::new(384_000.0)))
    }

    fn sample_lane() -> Lane {
        Lane::Sample(SampleBuffer::zeros(
            2,
            SampleRate::new(48_000.0),
            BitDepth::new(24),
            ClockDomainId(0),
        ))
    }

    #[test]
    fn domain_follows_the_variant() {
        assert_eq!(voltage_lane().domain(), Domain::Analog);
        assert_eq!(sample_lane().domain(), Domain::DigitalAudio);
    }

    #[test]
    fn len_and_is_empty_delegate_to_the_buffer() {
        assert_eq!(voltage_lane().len(), 4);
        assert_eq!(sample_lane().len(), 2);
        assert!(!voltage_lane().is_empty());
        assert!(Lane::Voltage(VoltageBuffer::zeros(0, AnalogRate::new(384_000.0))).is_empty());
    }

    #[test]
    fn typed_accessors_return_the_inner_buffer() {
        assert_eq!(voltage_lane().voltage().len(), 4);
        assert_eq!(sample_lane().sample().len(), 2);

        let mut v = voltage_lane();
        v.voltage_mut().fill(crate::signal::Volts::new(1.0));
        assert!(v.voltage().as_slice().iter().all(|&s| s == 1.0));
    }

    #[test]
    #[should_panic(expected = "lane is not Voltage")]
    fn voltage_on_a_sample_lane_is_unreachable() {
        let _ = sample_lane().voltage();
    }

    #[test]
    #[should_panic(expected = "lane is not Sample")]
    fn sample_on_a_voltage_lane_is_unreachable() {
        let _ = voltage_lane().sample();
    }
}
