//! The multitrack recorder: the record/playback stage of the `computer` DAW (Story 5.11).
//!
//! A digital node carrying an arbitrary number of **mono tracks**. It is deliberately **not** the
//! router — routing, monitoring, level faders, and summing live in a downstream [`Matrix`] crossbar
//! (the "simple mixer"). This node does only what a tape machine does: **record** and **play back**,
//! against the DAW [`Transport`] it owns.
//!
//! **Ports: N sends in → (N + T) lanes out.** The input port carries the N USB *sends* (the
//! interface's inputs, digitized). The output port carries **N + T** lanes: the N sends **passed
//! through** (so the downstream mixer can monitor a live input) followed by **T track playbacks**
//! (recorded material, silent unless the transport is rolling). The mixer therefore sees one bus of
//! every source it can route — live inputs and track playbacks — and folds them to the M returns with
//! per-crosspoint gains. Track count is independent of the interface's channel count: 30 tracks can
//! fold to a 2-lane master, and a track can fan out to several returns — both are just crosspoints in
//! the mixer, not properties of this node.
//!
//! **Per track:** it **records** its assigned send lane to a file (streamed out as raw PCM) while the
//! transport is rolling *and* record-enabled *and* the track is armed; and **plays back** its file
//! (streamed in as raw PCM) to its own output lane while rolling.
//!
//! **Overdub emerges from the per-track loop.** Playback and record are independent per-track work on
//! the one rolling playhead — a track plays its file while an armed track records, in the same
//! `process` call. There is no play-vs-record mode (see [`Transport`]'s overdub invariant).
//!
//! **Clock provenance.** The transport advances by the **runtime digital lane length** each block
//! (read off the buffers), never a hardcoded rate — the DAW follows whatever rate the interface clocks
//! the lanes at (see `transport.rs`).
//!
//! **The host is dumb byte storage.** The only thing crossing the sim↔host boundary is opaque **raw
//! PCM bytes** through the per-track [`ByteRing`]s: the host drains each track's `record` ring to a WAV
//! file on disk and fills its `playback` ring from one. The WAV *header* is a file-lifecycle concern
//! handled at the file boundary (host + the `wav` codec), not streamed per block; the ring carries the
//! payload — one little-endian `f32` frame per sample, so a torn half-frame is impossible.
//!
//! **Hot-path.** `process` streams PCM inline (a 4-byte [`ByteRing`] read/write per sample) with no
//! allocation (rings pre-allocated at construction; the frame buffer is a stack `[u8; 4]`), no panic
//! (bounds via `get`/`get_mut`; ring transfers are total), and denormal-free arithmetic.
//!
//! [`Matrix`]: super::Matrix

use super::Node;
use crate::byte_ring::ByteRing;
use crate::param::Params;
use crate::port::{AudioFormat, DigitalFace, InputPort, OutputPort};
use crate::signal::{BitDepth, Lane, SampleRate};
use crate::transport::Transport;

/// A **multitrack recorder**: `n_sends` digital send lanes in → `n_sends + n_tracks` lanes out (the
/// sends passed through, then one playback lane per track), recording/playing each mono track's file
/// via a per-track [`ByteRing`] to the host, against the DAW [`Transport`] it owns. Routing and levels
/// are a downstream [`Matrix`](super::Matrix)'s job. See the module docs for the full model.
pub struct MultitrackRecorder {
    n_sends: usize,
    n_tracks: usize,
    /// Per-track assigned send lane — the source it records.
    input: Vec<usize>,
    /// Per-track record-arm: only armed tracks capture (and only while the transport records).
    armed: Vec<bool>,
    /// Per-track playback PCM fed by the host (drained per sample while rolling).
    inbound: Vec<ByteRing>,
    /// Per-track recorded PCM for the host to drain to a file (filled per sample while recording).
    outbound: Vec<ByteRing>,
    /// The DAW transport — this node's own digital-domain clock (rolling/record/playhead).
    transport: Transport,
    inputs: Vec<InputPort>,
    outputs: Vec<OutputPort>,
}

impl MultitrackRecorder {
    /// Bytes reserved per track per direction for the in-flight PCM stream. 32 KiB ≈ 8k `f32` frames
    /// ≈ 64 blocks of 128 — ample slack for the host to service the rings a few blocks behind without
    /// under/overrunning at human transport rates.
    const STREAM_RING_BYTES: usize = 1 << 15;

    /// A recorder with `n_sends` send lanes in and `n_tracks` mono tracks, all ports at `rate`/`bits`.
    /// Its output carries `n_sends + n_tracks` lanes (the sends passed through, then the track
    /// playbacks). Tracks default to recording their own send (track `t` → send `min(t, n_sends−1)`),
    /// disarmed.
    ///
    /// # Panics
    /// Panics unless `n_sends ≥ 1` and `n_tracks ≥ 1`. Construction-time.
    #[must_use]
    pub fn new(rate: SampleRate, bits: BitDepth, n_sends: usize, n_tracks: usize) -> Self {
        assert!(
            n_sends >= 1 && n_tracks >= 1,
            "MultitrackRecorder needs ≥1 send and track (got {n_sends}/{n_tracks})"
        );

        let input_face = DigitalFace::new(AudioFormat::new(rate, bits, n_sends as u16));
        let output_face =
            DigitalFace::new(AudioFormat::new(rate, bits, (n_sends + n_tracks) as u16));

        Self {
            n_sends,
            n_tracks,
            input: (0..n_tracks).map(|t| t.min(n_sends - 1)).collect(),
            armed: vec![false; n_tracks],
            inbound: (0..n_tracks)
                .map(|_| ByteRing::with_capacity(Self::STREAM_RING_BYTES))
                .collect(),
            outbound: (0..n_tracks)
                .map(|_| ByteRing::with_capacity(Self::STREAM_RING_BYTES))
                .collect(),
            transport: Transport::new(),
            inputs: vec![input_face.into()],
            outputs: vec![output_face.into()],
        }
    }

    /// Number of tracks.
    #[must_use]
    pub fn track_count(&self) -> usize {
        self.n_tracks
    }

    /// Number of send (input) lanes.
    #[must_use]
    pub fn send_count(&self) -> usize {
        self.n_sends
    }

    /// The output lane index carrying track `track`'s playback: `n_sends + track` (after the passed-
    /// through sends). The downstream mixer routes this lane; a device catalog uses it to wire the
    /// crossbar.
    #[must_use]
    pub fn playback_lane(&self, track: usize) -> usize {
        self.n_sends + track
    }

    /// The DAW transport (read).
    #[must_use]
    pub fn transport(&self) -> &Transport {
        &self.transport
    }

    /// The DAW transport (drive it — play/stop/seek/record-enable).
    pub fn transport_mut(&mut self) -> &mut Transport {
        &mut self.transport
    }

    /// Assign track `track`'s record source to send lane `lane`. No-op for an out-of-range track.
    pub fn set_input(&mut self, track: usize, lane: usize) {
        if let Some(slot) = self.input.get_mut(track) {
            *slot = lane;
        }
    }

    /// Arm or disarm track `track` for recording.
    pub fn set_armed(&mut self, track: usize, armed: bool) {
        if let Some(slot) = self.armed.get_mut(track) {
            *slot = armed;
        }
    }

    /// Track `track`'s assigned send lane (for tests/inspection).
    #[must_use]
    pub fn input_of(&self, track: usize) -> Option<usize> {
        self.input.get(track).copied()
    }

    /// Whether track `track` is armed.
    #[must_use]
    pub fn is_armed(&self, track: usize) -> bool {
        self.armed.get(track).copied().unwrap_or(false)
    }

    /// Track `track`'s **playback** ring — the host fills it with raw PCM bytes ahead of the playhead.
    pub fn playback_ring_mut(&mut self, track: usize) -> Option<&mut ByteRing> {
        self.inbound.get_mut(track)
    }

    /// Track `track`'s **record** ring — the host drains raw PCM bytes from it to a file on disk.
    pub fn record_ring_mut(&mut self, track: usize) -> Option<&mut ByteRing> {
        self.outbound.get_mut(track)
    }
}

impl Node for MultitrackRecorder {
    fn inputs(&self) -> &[InputPort] {
        &self.inputs
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    fn process(&mut self, _params: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
        // Digital lane length this block (block_len / M for the interface's rate). Advancing the
        // transport by this — not a constant — is what makes the DAW follow the interface clock.
        let digital_len = outputs.first().map_or(0, |l| l.sample().len());
        let n_sends = self.n_sends;
        let rolling = self.transport.is_rolling();

        // Fill the output bus: lanes 0..N mirror the live sends (for the mixer to monitor); lanes
        // N..N+T carry each track's playback (its file while rolling, else silence).
        for (lane, out) in outputs.iter_mut().enumerate() {
            let dst = out.sample_mut().as_mut_slice();
            if lane < n_sends {
                match inputs.get(lane) {
                    Some(src) => {
                        for (d, &s) in dst.iter_mut().zip(src.sample().as_slice()) {
                            *d = s;
                        }
                    }
                    None => dst.fill(0.0),
                }
            } else {
                let ring = &mut self.inbound[lane - n_sends];
                for d in dst.iter_mut() {
                    *d = if rolling {
                        let mut frame = [0u8; 4];
                        if ring.read(&mut frame) {
                            f32::from_le_bytes(frame)
                        } else {
                            0.0
                        }
                    } else {
                        0.0
                    };
                }
            }
        }

        // Record: while the transport is recording, each armed track captures its assigned send to
        // its file stream. A full ring (host draining behind) drops the frame whole — an honest gap.
        if self.transport.is_recording() {
            for t in 0..self.n_tracks {
                if !self.armed[t] {
                    continue;
                }
                let in_lane = self.input[t].min(n_sends - 1);
                let Some(src) = inputs.get(in_lane) else {
                    continue;
                };
                let outbound = &mut self.outbound[t];
                for &x in src.sample().as_slice() {
                    outbound.write(&x.to_le_bytes());
                }
            }
        }

        // One digital block consumed — advance the transport (a no-op while stopped).
        self.transport.advance(digital_len as u64);
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
        BitDepth::new(24)
    }

    /// `vals`-per-lane constant send lanes, `len` samples each.
    fn sends(vals: &[f32], len: usize) -> Vec<Lane> {
        vals.iter()
            .map(|&v| {
                Lane::Sample(SampleBuffer::from_samples(
                    vec![v; len],
                    fs(),
                    bits(),
                    ClockDomainId::SINGLE,
                ))
            })
            .collect()
    }

    /// `n` zeroed output lanes.
    fn out_lanes(n: usize, len: usize) -> Vec<Lane> {
        (0..n)
            .map(|_| {
                Lane::Sample(SampleBuffer::zeros(
                    len,
                    fs(),
                    bits(),
                    ClockDomainId::SINGLE,
                ))
            })
            .collect()
    }

    /// Raw little-endian `f32` PCM bytes, as the host feeds/drains through the rings.
    fn pcm(samples: &[f32]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_le_bytes()).collect()
    }

    fn unpcm(bytes: &[u8]) -> Vec<f32> {
        bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }

    #[test]
    fn declares_n_sends_in_and_n_plus_t_lanes_out_no_params() {
        let r = MultitrackRecorder::new(fs(), bits(), 8, 3);
        assert_eq!(r.inputs().len(), 1);
        assert_eq!(r.outputs().len(), 1);
        assert_eq!(r.inputs()[0].lane_count(), 8, "8 sends in");
        assert_eq!(
            r.outputs()[0].lane_count(),
            8 + 3,
            "sends passed through + 3 playbacks"
        );
        assert_eq!(r.inputs()[0].domain(), Domain::DigitalAudio);
        // Routing/levels are the Matrix's job; the recorder exposes no control params.
        assert_eq!(r.params().len(), 0);
        assert_eq!(r.playback_lane(0), 8);
        assert_eq!(r.playback_lane(2), 10);
    }

    #[test]
    fn passes_the_live_sends_through_to_the_first_n_output_lanes() {
        // The mixer monitors live inputs off these passthrough lanes.
        let mut r = MultitrackRecorder::new(fs(), bits(), 2, 1);
        let ins = sends(&[0.3, -0.6], 8);
        let mut outs = out_lanes(2 + 1, 8);
        r.process(&Params::EMPTY, &ins, &mut outs);
        assert!(
            outs[0].sample().as_slice().iter().all(|&s| s == 0.3),
            "send 0 passed through"
        );
        assert!(
            outs[1].sample().as_slice().iter().all(|&s| s == -0.6),
            "send 1 passed through"
        );
    }

    #[test]
    fn a_track_playback_lane_is_silent_until_rolling() {
        let mut r = MultitrackRecorder::new(fs(), bits(), 1, 1);
        let take = [0.1, 0.2, 0.3, 0.4];
        r.playback_ring_mut(0).unwrap().write(&pcm(&take));
        let ins = sends(&[0.0], 4);

        // Stopped: the playback lane (index n_sends = 1) is silent, the ring untouched.
        let mut outs = out_lanes(1 + 1, 4);
        r.process(&Params::EMPTY, &ins, &mut outs);
        assert!(outs[1].sample().as_slice().iter().all(|&s| s == 0.0));

        // Rolling: the fed file appears on the playback lane, in order.
        r.transport_mut().play();
        let mut outs = out_lanes(1 + 1, 4);
        r.process(&Params::EMPTY, &ins, &mut outs);
        assert_eq!(outs[1].sample().as_slice(), &take);
    }

    #[test]
    fn records_the_armed_send_only_when_rolling_and_record_enabled() {
        let mut r = MultitrackRecorder::new(fs(), bits(), 1, 1);
        r.set_armed(0, true);
        let ins = sends(&[0.5], 4);

        // Armed but stopped ⇒ nothing captured.
        let mut outs = out_lanes(2, 4);
        r.process(&Params::EMPTY, &ins, &mut outs);
        assert_eq!(r.record_ring_mut(0).unwrap().len(), 0);

        // Rolling + record-enabled + armed ⇒ the send is captured.
        r.transport_mut().play();
        r.transport_mut().set_record_enabled(true);
        let mut outs = out_lanes(2, 4);
        r.process(&Params::EMPTY, &ins, &mut outs);
        let mut bytes = vec![0u8; 16];
        assert!(r.record_ring_mut(0).unwrap().read(&mut bytes));
        assert_eq!(unpcm(&bytes), vec![0.5; 4]);
    }

    /// The overdub oracle: track 0 plays back its file **while** track 1 records its send — same
    /// `process` call. Track 0's playback lane carries its file (untouched by the concurrent record),
    /// and track 1's record ring fills with its assigned send.
    #[test]
    fn overdub_plays_one_track_while_recording_another() {
        let mut r = MultitrackRecorder::new(fs(), bits(), 2, 2);
        // Track 0 plays a file; track 1 records send 1.
        let take = [0.1, 0.2, 0.3, 0.4];
        r.playback_ring_mut(0).unwrap().write(&pcm(&take));
        r.set_input(1, 1);
        r.set_armed(1, true);

        r.transport_mut().play();
        r.transport_mut().set_record_enabled(true);

        let ins = sends(&[0.0, 0.9], 4); // send 1 = the source being overdubbed
        let mut outs = out_lanes(2 + 2, 4);
        r.process(&Params::EMPTY, &ins, &mut outs);

        // Track 0's playback lane (index n_sends + 0 = 2) carries the file, unaffected by the record.
        assert_eq!(outs[2].sample().as_slice(), &take, "track 0 plays its file");
        // Track 1 captured its send.
        let mut bytes = vec![0u8; 16];
        assert!(r.record_ring_mut(1).unwrap().read(&mut bytes));
        assert_eq!(unpcm(&bytes), vec![0.9; 4], "track 1 recorded its send");
    }

    #[test]
    fn transport_advances_by_the_runtime_lane_length() {
        let mut r = MultitrackRecorder::new(fs(), bits(), 1, 1);
        let ins = sends(&[0.0], 128);
        let mut outs = out_lanes(2, 128);

        r.process(&Params::EMPTY, &ins, &mut outs);
        assert_eq!(r.transport().playhead(), 0, "stopped ⇒ held");

        r.transport_mut().play();
        r.process(&Params::EMPTY, &ins, &mut outs);
        r.process(&Params::EMPTY, &ins, &mut outs);
        assert_eq!(r.transport().playhead(), 256, "two 128-sample blocks");
    }
}
