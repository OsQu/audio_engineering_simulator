//! The multitrack recorder: the track-channel stage of the `computer` DAW (Story 5.11).
//!
//! A digital node carrying an arbitrary number of **mono tracks**, each a DAW **channel strip**: its
//! source is its recorded playback plus (when monitoring) its assigned live input, scaled by a
//! per-track **fader**. The node's output is one **post-fader track channel per track** — so a
//! downstream meter reads each track's after-fader level, and a downstream [`Matrix`] crossbar routes
//! the channels to the return buses. It is **not** the router or the mixer bus: routing/summing live
//! in that `Matrix`; only the per-**track** fader lives here (there is no per-input fader — trim a
//! live input at the preamp, as on a real desk).
//!
//! **Ports: N sends in → T track channels out.** The input port carries the N USB *sends* (the
//! interface's inputs, digitized), used as record + monitor sources. The output carries **T** lanes,
//! one post-fader channel per track. Track count is independent of the interface's channel count.
//!
//! **Per track, every block:** the channel signal is `(playback + monitored_send) × fader`, written
//! to its output lane; and, while the transport is rolling *and* record-enabled *and* the track is
//! armed, its assigned send is **recorded** to a file (streamed out as raw PCM). Monitoring and the
//! fader are inside the channel — so an armed track hears its input *through its fader*, exactly like
//! a console.
//!
//! **Overdub emerges from the per-track loop.** Playback and record are independent per-track work on
//! the one rolling playhead — a track plays its file while an armed track records, in the same
//! `process` call. There is no play-vs-record mode (see [`Transport`]'s overdub invariant).
//!
//! **The fader is a recorder-owned [`Smoother`]**, set over the DAW control seam
//! ([`set_track_level`](MultitrackRecorder::set_track_level)) alongside transport/arm/monitor — not an
//! exposed control param — and de-zippered with the framework smoother (reused, not reimplemented), so
//! a level change never clicks. It advances once per block on the recorder's digital rate.
//!
//! **Clock provenance.** The transport advances by the **runtime digital lane length** each block,
//! never a hardcoded rate — the DAW follows whatever rate the interface clocks the lanes at.
//!
//! **The host is dumb byte storage.** The only sim↔host data is opaque **raw PCM bytes** through the
//! per-track [`ByteRing`]s: the host drains each track's `record` ring to a WAV file and fills its
//! `playback` ring from one; the WAV header is a file-lifecycle concern (host + the `wav` codec), the
//! ring carries one little-endian `f32` frame per sample so a torn half-frame is impossible.
//!
//! **Hot-path.** `process` streams PCM inline (a 4-byte [`ByteRing`] read/write per sample) and reads
//! the fader per sample, with no allocation (rings + smoothers pre-allocated; the frame buffer is a
//! stack `[u8; 4]`), no panic (bounds via `get`/`get_mut`; ring transfers are total).
//!
//! [`Matrix`]: super::Matrix
//! [`Smoother`]: crate::param::Smoother

use super::Node;
use crate::byte_ring::ByteRing;
use crate::param::{Params, Smoother, smooth_samples};
use crate::port::{AudioFormat, DigitalFace, InputPort, OutputPort};
use crate::signal::{BitDepth, Lane, SampleRate};
use crate::transport::Transport;

/// A **multitrack recorder**: `n_sends` digital send lanes in → `n_tracks` post-fader track channels
/// out. Each mono track records/plays a file via a per-track [`ByteRing`] to the host and scales its
/// (playback + monitored) signal by a per-track fader, against the DAW [`Transport`] it owns. Routing
/// and bus summing are a downstream [`Matrix`](super::Matrix)'s job. See the module docs.
pub struct MultitrackRecorder {
    n_sends: usize,
    n_tracks: usize,
    /// Per-track assigned send lane — the source it records and (when monitoring) hears.
    input: Vec<usize>,
    /// Per-track record-arm: only armed tracks capture (and only while the transport records).
    armed: Vec<bool>,
    /// Per-track input monitoring: pass the assigned send through the channel (and its fader).
    monitoring: Vec<bool>,
    /// Per-track fader — a de-zippered level, driven over the control seam (not an exposed param).
    level: Vec<Smoother>,
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
    /// Largest fader gain (+12 dB of makeup) and its de-zipper glide — matching [`Matrix`]'s.
    ///
    /// [`Matrix`]: super::Matrix
    const MAX_GAIN: f32 = 4.0;
    const SMOOTH_MS: f32 = 5.0;

    /// Bytes reserved per track per direction for the in-flight PCM stream. 32 KiB ≈ 8k `f32` frames
    /// ≈ 64 blocks of 128 — ample slack for the host to service the rings a few blocks behind.
    const STREAM_RING_BYTES: usize = 1 << 15;

    /// A recorder with `n_sends` send lanes in and `n_tracks` mono track channels out, all ports at
    /// `rate`/`bits`. Tracks default to input-monitoring their own send (track `t` → send
    /// `min(t, n_sends−1)`) at **unity** fader, disarmed — so a fresh recorder passes its inputs to
    /// the mixer at unity, one channel per track.
    ///
    /// # Panics
    /// Panics unless `n_sends ≥ 1` and `n_tracks ≥ 1`. Construction-time.
    #[must_use]
    pub fn new(rate: SampleRate, bits: BitDepth, n_sends: usize, n_tracks: usize) -> Self {
        assert!(
            n_sends >= 1 && n_tracks >= 1,
            "MultitrackRecorder needs ≥1 send and track (got {n_sends}/{n_tracks})"
        );

        let glide = smooth_samples(Self::SMOOTH_MS, rate.as_hz());
        let input_face = DigitalFace::new(AudioFormat::new(rate, bits, n_sends as u16));
        let output_face = DigitalFace::new(AudioFormat::new(rate, bits, n_tracks as u16));

        Self {
            n_sends,
            n_tracks,
            input: (0..n_tracks).map(|t| t.min(n_sends - 1)).collect(),
            armed: vec![false; n_tracks],
            monitoring: vec![true; n_tracks],
            level: (0..n_tracks)
                .map(|_| Smoother::new(1.0, 0.0, Self::MAX_GAIN, glide))
                .collect(),
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

    /// The DAW transport (read).
    #[must_use]
    pub fn transport(&self) -> &Transport {
        &self.transport
    }

    /// The DAW transport (drive it — play/stop/seek/record-enable).
    pub fn transport_mut(&mut self) -> &mut Transport {
        &mut self.transport
    }

    /// Assign track `track`'s record/monitor source to send lane `lane`. No-op for a bad track.
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

    /// Enable or disable input monitoring for track `track`.
    pub fn set_monitoring(&mut self, track: usize, monitoring: bool) {
        if let Some(slot) = self.monitoring.get_mut(track) {
            *slot = monitoring;
        }
    }

    /// Set track `track`'s fader to `target` (de-zippered, clamped to `[0, MAX_GAIN]`). No-op for a
    /// bad track.
    pub fn set_track_level(&mut self, track: usize, target: f32) {
        if let Some(s) = self.level.get_mut(track) {
            s.set_target(target);
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

    /// Whether track `track` is input-monitoring.
    #[must_use]
    pub fn is_monitoring(&self, track: usize) -> bool {
        self.monitoring.get(track).copied().unwrap_or(false)
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
        let recording = self.transport.is_recording();

        // Each output lane is one track's post-fader channel.
        for (t, out) in outputs.iter_mut().enumerate() {
            let in_lane = self.input[t].min(n_sends - 1);
            let monitoring = self.monitoring[t];
            let send: &[f32] = inputs
                .get(in_lane)
                .map(|l| l.sample().as_slice())
                .unwrap_or(&[]);

            let dst = out.sample_mut().as_mut_slice();
            let inbound = &mut self.inbound[t];
            let level = &mut self.level[t];
            for (i, o) in dst.iter_mut().enumerate() {
                let play = if rolling {
                    let mut frame = [0u8; 4];
                    if inbound.read(&mut frame) {
                        f32::from_le_bytes(frame)
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };
                let monitored = if monitoring {
                    send.get(i).copied().unwrap_or(0.0)
                } else {
                    0.0
                };
                *o = (play + monitored) * level.value_at(i);
            }
            level.advance(digital_len);
        }

        // Record: while recording, each armed track captures its assigned send (pre-fader — the source
        // as it arrives). A full ring (host draining behind) drops the frame whole — an honest gap.
        if recording {
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

    fn tracks_out(t: usize, len: usize) -> Vec<Lane> {
        (0..t)
            .map(|_| Lane::Sample(SampleBuffer::zeros(len, fs(), bits(), ClockDomainId::SINGLE)))
            .collect()
    }

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
    fn declares_n_sends_in_and_t_track_channels_out_no_params() {
        let r = MultitrackRecorder::new(fs(), bits(), 8, 3);
        assert_eq!(r.inputs().len(), 1);
        assert_eq!(r.outputs().len(), 1);
        assert_eq!(r.inputs()[0].lane_count(), 8, "8 sends in");
        assert_eq!(r.outputs()[0].lane_count(), 3, "3 track channels out");
        assert_eq!(r.inputs()[0].domain(), Domain::DigitalAudio);
        // The fader is a recorder-owned smoother driven over the control seam, not an exposed param.
        assert_eq!(r.params().len(), 0);
    }

    #[test]
    fn a_default_track_monitors_its_send_at_unity() {
        // Fresh recorder: track t monitors send t at unity fader → its channel carries the input.
        let mut r = MultitrackRecorder::new(fs(), bits(), 2, 2);
        let ins = sends(&[0.3, -0.6], 8);
        let mut outs = tracks_out(2, 8);
        r.process(&Params::EMPTY, &ins, &mut outs);
        assert!(outs[0].sample().as_slice().iter().all(|&s| s == 0.3), "track 0 = send 0");
        assert!(outs[1].sample().as_slice().iter().all(|&s| s == -0.6), "track 1 = send 1");
    }

    #[test]
    fn monitoring_gate_passes_or_silences_the_channel() {
        let mut r = MultitrackRecorder::new(fs(), bits(), 1, 1);
        let ins = sends(&[0.6], 8);

        let mut outs = tracks_out(1, 8);
        r.process(&Params::EMPTY, &ins, &mut outs);
        assert!(outs[0].sample().as_slice().iter().all(|&s| s == 0.6));

        r.set_monitoring(0, false);
        let mut outs = tracks_out(1, 8);
        r.process(&Params::EMPTY, &ins, &mut outs);
        assert!(
            outs[0].sample().as_slice().iter().all(|&s| s == 0.0),
            "monitoring off ⇒ silent channel"
        );
    }

    #[test]
    fn the_track_fader_scales_the_channel_once_settled() {
        // Set the fader to 0.5 and run enough blocks for the 5 ms glide (240 samples) to settle,
        // then the monitored input comes out halved.
        let mut r = MultitrackRecorder::new(fs(), bits(), 1, 1);
        r.set_track_level(0, 0.5);
        let ins = sends(&[0.8], 128);
        let mut outs = tracks_out(1, 128);
        for _ in 0..4 {
            r.process(&Params::EMPTY, &ins, &mut outs); // 4×128 = 512 ≫ 240-sample glide
        }
        assert!(
            outs[0].sample().as_slice().iter().all(|&s| (s - 0.4).abs() < 1e-6),
            "0.8 · 0.5 = 0.4 after the fader settles"
        );
    }

    #[test]
    fn playback_streams_the_file_through_the_channel_while_rolling() {
        // Monitoring off, feed a file, roll → the channel carries the playback (at unity fader).
        let mut r = MultitrackRecorder::new(fs(), bits(), 1, 1);
        r.set_monitoring(0, false);
        let take = [0.1, 0.2, 0.3, 0.4];
        r.playback_ring_mut(0).unwrap().write(&pcm(&take));
        let ins = sends(&[0.0], 4);

        let mut outs = tracks_out(1, 4);
        r.process(&Params::EMPTY, &ins, &mut outs);
        assert!(outs[0].sample().as_slice().iter().all(|&s| s == 0.0), "stopped ⇒ silent");

        r.transport_mut().play();
        let mut outs = tracks_out(1, 4);
        r.process(&Params::EMPTY, &ins, &mut outs);
        assert_eq!(outs[0].sample().as_slice(), &take);
    }

    #[test]
    fn records_the_armed_send_only_when_rolling_and_record_enabled() {
        let mut r = MultitrackRecorder::new(fs(), bits(), 1, 1);
        r.set_armed(0, true);
        let ins = sends(&[0.5], 4);

        let mut outs = tracks_out(1, 4);
        r.process(&Params::EMPTY, &ins, &mut outs);
        assert_eq!(r.record_ring_mut(0).unwrap().len(), 0, "stopped ⇒ nothing captured");

        r.transport_mut().play();
        r.transport_mut().set_record_enabled(true);
        let mut outs = tracks_out(1, 4);
        r.process(&Params::EMPTY, &ins, &mut outs);
        let mut bytes = vec![0u8; 16];
        assert!(r.record_ring_mut(0).unwrap().read(&mut bytes));
        assert_eq!(unpcm(&bytes), vec![0.5; 4], "records the pre-fader send");
    }

    /// The overdub oracle: track 0 plays its file through its channel **while** track 1 records its
    /// send — same `process`. Track 0's channel carries its file (unaffected), track 1's record ring
    /// fills with its send.
    #[test]
    fn overdub_plays_one_track_while_recording_another() {
        let mut r = MultitrackRecorder::new(fs(), bits(), 2, 2);
        r.set_monitoring(0, false); // track 0 plays back only
        let take = [0.1, 0.2, 0.3, 0.4];
        r.playback_ring_mut(0).unwrap().write(&pcm(&take));
        r.set_input(1, 1);
        r.set_armed(1, true);

        r.transport_mut().play();
        r.transport_mut().set_record_enabled(true);

        let ins = sends(&[0.0, 0.9], 4);
        let mut outs = tracks_out(2, 4);
        r.process(&Params::EMPTY, &ins, &mut outs);

        assert_eq!(outs[0].sample().as_slice(), &take, "track 0 plays its file");
        let mut bytes = vec![0u8; 16];
        assert!(r.record_ring_mut(1).unwrap().read(&mut bytes));
        assert_eq!(unpcm(&bytes), vec![0.9; 4], "track 1 recorded its send");
    }

    #[test]
    fn transport_advances_by_the_runtime_lane_length() {
        let mut r = MultitrackRecorder::new(fs(), bits(), 1, 1);
        let ins = sends(&[0.0], 128);
        let mut outs = tracks_out(1, 128);

        r.process(&Params::EMPTY, &ins, &mut outs);
        assert_eq!(r.transport().playhead(), 0, "stopped ⇒ held");

        r.transport_mut().play();
        r.process(&Params::EMPTY, &ins, &mut outs);
        r.process(&Params::EMPTY, &ins, &mut outs);
        assert_eq!(r.transport().playhead(), 256, "two 128-sample blocks");
    }
}
