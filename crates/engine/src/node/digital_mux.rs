//! Multichannel digital (de)multiplexers: bundle N mono digital channels into one N-lane stream, and
//! split it back out. The seam that lets one physical connector (USB, ADAT, S/PDIF) carry many
//! channels while the DSP nodes on either side stay mono.

use super::Node;
use crate::param::Params;
use crate::port::{AudioFormat, DigitalFace, InputPort, OutputPort};
use crate::signal::{BitDepth, Lane, SampleRate};

/// A **digital multiplexer**: `n` mono digital inputs → one `n`-channel digital output. All ports
/// share the same `rate`/`bits`; only the channel count differs (1 per input, `n` on the output). It
/// carries no clock or electrical semantics — each output channel is an exact sample copy of the
/// corresponding input, so the machinery is a rechannelize, not a transform.
///
/// This is how a device presents many channels over **one connector**: N converter outputs feed a mux
/// whose single N-lane port is the "USB send" jack. The dual is [`DigitalDemux`].
///
/// `n` inputs; one output.
pub struct DigitalMux {
    inputs: Vec<InputPort>,
    outputs: [OutputPort; 1],
}

impl DigitalMux {
    /// A mux bundling `channels` mono `rate`/`bits` inputs into one `channels`-wide output.
    ///
    /// # Panics
    /// Panics if `channels == 0` — a mux needs at least one channel. Checked at construction.
    #[must_use]
    pub fn new(rate: SampleRate, bits: BitDepth, channels: u16) -> Self {
        assert!(channels >= 1, "DigitalMux needs at least one channel");
        let mono = DigitalFace::new(AudioFormat::new(rate, bits, 1));
        Self {
            inputs: (0..channels).map(|_| mono.into()).collect(),
            outputs: [DigitalFace::new(AudioFormat::new(rate, bits, channels)).into()],
        }
    }
}

impl Node for DigitalMux {
    fn inputs(&self) -> &[InputPort] {
        &self.inputs
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    fn process(&mut self, _params: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
        // Ports map to lanes in port-then-lane order: the `n` mono inputs are `n` input lanes, and the
        // one `n`-channel output port owns `n` output lanes — so channel `i` is a straight lane copy.
        // An unconnected input reads silence, which copies through as digital zero.
        for (out_lane, in_lane) in outputs.iter_mut().zip(inputs) {
            out_lane
                .sample_mut()
                .as_mut_slice()
                .copy_from_slice(in_lane.sample().as_slice());
        }
    }
}

/// A **digital demultiplexer**: one `n`-channel digital input → `n` mono digital outputs. The dual of
/// [`DigitalMux`] — it unpacks a multichannel stream (a "USB return") back into per-channel wires for
/// the mono DSP/DA nodes downstream. Exact sample copies; same `rate`/`bits` throughout.
///
/// One input; `n` outputs.
pub struct DigitalDemux {
    inputs: [InputPort; 1],
    outputs: Vec<OutputPort>,
}

impl DigitalDemux {
    /// A demux splitting one `channels`-wide `rate`/`bits` input into `channels` mono outputs.
    ///
    /// # Panics
    /// Panics if `channels == 0` — a demux needs at least one channel. Checked at construction.
    #[must_use]
    pub fn new(rate: SampleRate, bits: BitDepth, channels: u16) -> Self {
        assert!(channels >= 1, "DigitalDemux needs at least one channel");
        let mono = DigitalFace::new(AudioFormat::new(rate, bits, 1));
        Self {
            inputs: [DigitalFace::new(AudioFormat::new(rate, bits, channels)).into()],
            outputs: (0..channels).map(|_| mono.into()).collect(),
        }
    }
}

impl Node for DigitalDemux {
    fn inputs(&self) -> &[InputPort] {
        &self.inputs
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    fn process(&mut self, _params: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
        // The one `n`-channel input port owns `n` input lanes; the `n` mono outputs are `n` output
        // lanes — channel `i` is a straight lane copy, the mirror of the mux.
        for (out_lane, in_lane) in outputs.iter_mut().zip(inputs) {
            out_lane
                .sample_mut()
                .as_mut_slice()
                .copy_from_slice(in_lane.sample().as_slice());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::{ClockDomainId, Domain, SampleBuffer};

    fn fs() -> SampleRate {
        SampleRate::new(48_000.0)
    }
    fn bits() -> BitDepth {
        BitDepth::new(16)
    }

    #[test]
    fn mux_declares_n_mono_ins_one_n_channel_out() {
        let m = DigitalMux::new(fs(), bits(), 4);
        assert_eq!(m.inputs().len(), 4);
        for p in m.inputs() {
            assert_eq!(p.domain(), Domain::DigitalAudio);
            assert_eq!(p.lane_count(), 1);
        }
        assert_eq!(m.outputs().len(), 1);
        assert_eq!(m.outputs()[0].lane_count(), 4);
        assert_eq!(m.outputs()[0].digital().unwrap().format().channels(), 4);
    }

    #[test]
    fn demux_declares_one_n_channel_in_n_mono_outs() {
        let d = DigitalDemux::new(fs(), bits(), 4);
        assert_eq!(d.inputs().len(), 1);
        assert_eq!(d.inputs()[0].lane_count(), 4);
        assert_eq!(d.outputs().len(), 4);
        for p in d.outputs() {
            assert_eq!(p.lane_count(), 1);
        }
    }

    /// A block of distinct per-channel samples survives mux → demux **bit-exact**: each mono input
    /// lane arrives on the matching mono output lane unchanged.
    #[test]
    fn mux_then_demux_round_trips_each_channel() {
        let n = 3usize;
        let len = 8;
        // Distinct constant per channel so a cross-wire would show.
        let vals = [0.25_f32, -0.5, 0.75];

        // Mux: n mono input lanes → one n-channel output (n lanes).
        let mut mux = DigitalMux::new(fs(), bits(), n as u16);
        let ins: Vec<Lane> = (0..n)
            .map(|c| {
                Lane::Sample(SampleBuffer::from_samples(
                    vec![vals[c]; len],
                    fs(),
                    bits(),
                    ClockDomainId::SINGLE,
                ))
            })
            .collect();
        let mut muxed: Vec<Lane> = (0..n)
            .map(|_| {
                Lane::Sample(SampleBuffer::zeros(
                    len,
                    fs(),
                    bits(),
                    ClockDomainId::SINGLE,
                ))
            })
            .collect();
        mux.process(&Params::EMPTY, &ins, &mut muxed);

        // Demux: the n-channel stream (n lanes) → n mono outputs (n lanes).
        let mut demux = DigitalDemux::new(fs(), bits(), n as u16);
        let mut outs: Vec<Lane> = (0..n)
            .map(|_| {
                Lane::Sample(SampleBuffer::zeros(
                    len,
                    fs(),
                    bits(),
                    ClockDomainId::SINGLE,
                ))
            })
            .collect();
        demux.process(&Params::EMPTY, &muxed, &mut outs);

        for (c, out) in outs.iter().enumerate() {
            assert!(
                out.sample().as_slice().iter().all(|&s| s == vals[c]),
                "channel {c} should round-trip to {} bit-exact",
                vals[c]
            );
        }
    }

    #[test]
    #[should_panic(expected = "at least one channel")]
    fn mux_rejects_zero_channels() {
        let _ = DigitalMux::new(fs(), bits(), 0);
    }
}
