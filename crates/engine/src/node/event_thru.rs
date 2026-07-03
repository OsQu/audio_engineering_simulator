//! A control-event pass-through: copies its event input to its event output, unchanged.

use super::Node;
use crate::param::Params;
use crate::port::{EventFace, InputPort, OutputPort};
use crate::signal::Lane;

/// A **control-event pass-through**: one event input, one event output, copied verbatim each block.
///
/// It is the degenerate case of an event processor — the family whose non-trivial members transform
/// the stream (an arpeggiator, a MIDI channel filter). Here the transform is the identity, which is
/// exactly what a **standalone MIDI controller** is: a device that *produces* a performance by
/// forwarding what arrives at its input to its output (the physical "keys → MIDI-OUT", or a
/// thru/merge box). The performance itself is not modeled inside the box — it enters through the
/// node's **open event input** (host-fed when a human plays the focused device, edge-fed when a
/// cable is patched), the same source-agnostic "performance in" every event-consuming device has.
/// So a controller is this node with a MIDI-IN/MIDI-OUT face; no keyboard node, no internal
/// keys→voice edge (that would import MIDI where a device has none — PROJECT_PLAN §5 inside-the-box
/// boundary).
///
/// No analog or digital I/O, no params: it is pure event plumbing.
pub struct EventThru {
    inputs: [InputPort; 1],
    outputs: [OutputPort; 1],
}

impl EventThru {
    /// A pass-through whose input and output lanes each hold up to `capacity` events per block.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            inputs: [EventFace::new(capacity).into()],
            outputs: [EventFace::new(capacity).into()],
        }
    }
}

impl Node for EventThru {
    fn inputs(&self) -> &[InputPort] {
        &self.inputs
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    fn process(&mut self, _params: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
        // `copy_from` clears the output lane then copies up to its capacity — a producer owns its
        // output lane each block, so stale events never linger and the copy is bounded (hot-path
        // safe: no alloc, no panic).
        outputs[0].events_mut().copy_from(inputs[0].events());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::electrical::Ohms;
    use crate::graph::Graph;
    use crate::node::SynthVoice;
    use crate::schedule::{EventQueue, compile};
    use crate::signal::{AnalogRate, EventMessage, VoltageBuffer, Volts};
    use crate::test_util::rms;

    fn rate() -> AnalogRate {
        AnalogRate::new(384_000.0)
    }

    /// The controller's forwarder drives a downstream synth: a note-on injected into the pass-through's
    /// **open** event input reaches the voice through the `EventRoute` edge and makes it sound — the
    /// standalone-controller → synth path the UI exercises. (Without a controller the same note would be
    /// injected straight into the synth's open input; here it goes controller-in → controller-out →
    /// synth-in, one hop further, and must arrive intact.)
    #[test]
    fn forwards_a_note_to_a_downstream_voice() {
        let block = 16_384;
        let mut g = Graph::new();
        let ctrl = g.add(EventThru::new(64));
        let voice = g.add(SynthVoice::new(Volts::new(1.0), Ohms::new(1.0)));
        g.connect_ideal(ctrl, 0, voice, 0); // controller MIDI-OUT → synth MIDI-IN
        g.set_output(voice, 0);

        let mut sched = compile(g, block, rate(), 0).expect("valid controller→voice chain");
        // The controller's input is still open (nothing feeds it), so the host injects there.
        let ev = sched
            .event_input(ctrl, 0)
            .expect("the controller's open event input");
        let mut q = EventQueue::with_capacity(4);
        q.push(
            0,
            ev,
            EventMessage::NoteOn {
                note: 69, // A4 = 440 Hz
                velocity: 100,
            },
        );
        let mut out = VoltageBuffer::zeros(block, rate());
        sched.process_with_events(&mut out, &mut q);

        // The voice sounds — the note made it through the forwarder and the routing edge.
        let tail = &out.as_slice()[block / 4..];
        assert!(
            rms(tail) > 0.05,
            "the forwarded note must drive the voice, rms {}",
            rms(tail)
        );
    }
}
