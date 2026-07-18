//! Minimal WAV (RIFF/WAVE) reader/writer for the in-simulation DAW.
//!
//! The `computer` DAW (Story 5.11) records to and plays back from **WAV files on disk**, but the
//! host is only dumb byte storage — the *simulation* owns encode/decode, in the digital domain. So
//! this is a small, dependency-free, `wasm32`-safe codec, deliberately **not** the native-only
//! `hound` (which lives in the `harness` crate and never reaches the engine's wasm build).
//!
//! **Format: 32-bit IEEE-float PCM** (`WAVE_FORMAT_IEEE_FLOAT`, format tag 3) — the DAW's own
//! [`SampleBuffer`](crate::SampleBuffer) `f32` storage written straight to disk, so encode→decode is
//! **bit-exact** (no quantization step, unlike integer PCM). Mono today; `channels` leaves the door
//! open for interleaved stereo later. The header is the canonical 44-byte layout; the optional
//! `fact` chunk (nominally recommended for non-PCM formats) is omitted — no common reader needs it
//! and our own round-trip certainly doesn't.
//!
//! Streaming record uses [`wav_header`] with a placeholder length up front, appends
//! [`f32::to_le_bytes`] frames per block, then overwrites offset 0 with a corrected header at stop —
//! so a take never has to live whole in memory to be encoded. The whole-buffer [`encode_wav`] /
//! [`decode_wav`] pair serves the tests and any load-the-whole-file use (e.g. a host-side waveform
//! read for display).

/// Length in bytes of the canonical WAV header this codec writes (RIFF + `fmt ` + `data` preamble).
pub const WAV_HEADER_LEN: usize = 44;

/// `WAVE_FORMAT_IEEE_FLOAT` — samples are stored as raw little-endian `f32`.
const FMT_IEEE_FLOAT: u16 = 3;
/// Bits per sample for our `f32` storage.
const BITS_PER_SAMPLE: u16 = 32;
/// Bytes per sample (`f32`).
const BYTES_PER_SAMPLE: u32 = 4;

/// The format facts a WAV header carries that this codec cares about: rate and channel count.
///
/// Bit depth and sample format are fixed (32-bit IEEE float), so they aren't fields — they're the
/// codec's invariant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WavSpec {
    /// Sample rate in whole Hz (WAV stores an integer rate).
    pub sample_rate_hz: u32,
    /// Number of interleaved channels (1 = mono; the stereo door).
    pub channels: u16,
}

/// Why decoding a byte blob as WAV failed. Decoding is **total** — malformed or foreign bytes
/// (a real risk, since these come back from host file storage) yield an error, never a panic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WavError {
    /// Fewer than the 12-byte `RIFF … WAVE` preamble.
    TooShort,
    /// The first four bytes were not `RIFF`.
    BadRiffMagic,
    /// Bytes 8..12 were not `WAVE`.
    BadWaveMagic,
    /// No `fmt ` chunk, or one too short to read.
    MissingFmtChunk,
    /// The `fmt ` chunk's format tag was not 3 (IEEE float); the wrapped value is the tag found.
    UnsupportedFormat(u16),
    /// The `fmt ` chunk's bits-per-sample was not 32; the wrapped value is what was found.
    UnsupportedBits(u16),
    /// No `data` chunk was present.
    MissingDataChunk,
    /// A chunk's declared size ran past the end of the buffer.
    TruncatedData,
}

/// Build the 44-byte WAV header for `spec` declaring `data_bytes` of PCM payload to follow.
///
/// For streaming record, write this once with a placeholder (`data_bytes = 0`), stream the frames,
/// then recompute it with the true length and overwrite offset 0 at stop. `data_bytes` is the size
/// of the PCM payload only (frames × channels × 4).
#[must_use]
pub fn wav_header(spec: WavSpec, data_bytes: u32) -> [u8; WAV_HEADER_LEN] {
    let ch = spec.channels;
    let rate = spec.sample_rate_hz;
    let byte_rate = rate
        .saturating_mul(u32::from(ch))
        .saturating_mul(BYTES_PER_SAMPLE);
    let block_align = ch.saturating_mul(BYTES_PER_SAMPLE as u16); // channels × 4
    // RIFF chunk size covers everything after the first 8 bytes: 4 (WAVE) + 8+16 (fmt ) + 8 (data
    // header) + data_bytes = 36 + data_bytes.
    let riff_size = 36u32.saturating_add(data_bytes);

    let mut h = [0u8; WAV_HEADER_LEN];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&riff_size.to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes()); // fmt chunk body size
    h[20..22].copy_from_slice(&FMT_IEEE_FLOAT.to_le_bytes());
    h[22..24].copy_from_slice(&ch.to_le_bytes());
    h[24..28].copy_from_slice(&rate.to_le_bytes());
    h[28..32].copy_from_slice(&byte_rate.to_le_bytes());
    h[32..34].copy_from_slice(&block_align.to_le_bytes());
    h[34..36].copy_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_bytes.to_le_bytes());
    h
}

/// Encode `samples` (interleaved if multi-channel) as a complete WAV byte blob: header + PCM.
///
/// The one-shot counterpart to streaming — used by tests and any load-whole-file path. `f32`
/// samples are written verbatim as little-endian, so [`decode_wav`] recovers them bit-exactly.
#[must_use]
pub fn encode_wav(samples: &[f32], spec: WavSpec) -> Vec<u8> {
    let data_bytes = u32::try_from(samples.len().saturating_mul(4)).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(WAV_HEADER_LEN + samples.len() * 4);
    out.extend_from_slice(&wav_header(spec, data_bytes));
    for &s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// Decode a WAV byte blob into its samples and [`WavSpec`]. Total: any malformed input is a
/// [`WavError`], never a panic.
///
/// Walks the RIFF chunks (tolerating and skipping any we don't recognise), requiring a 32-bit
/// IEEE-float `fmt ` chunk and a `data` chunk. A `data` payload not a whole number of 4-byte frames
/// (a truncated file) is read up to its last complete frame.
///
/// # Errors
///
/// Returns a [`WavError`] if the bytes are too short, carry the wrong magic, lack a usable `fmt `/
/// `data` chunk, declare a non-float or non-32-bit format, or contain a chunk size past the buffer.
pub fn decode_wav(bytes: &[u8]) -> Result<(Vec<f32>, WavSpec), WavError> {
    if bytes.len() < 12 {
        return Err(WavError::TooShort);
    }
    if &bytes[0..4] != b"RIFF" {
        return Err(WavError::BadRiffMagic);
    }
    if &bytes[8..12] != b"WAVE" {
        return Err(WavError::BadWaveMagic);
    }

    let mut pos = 12;
    let mut spec: Option<WavSpec> = None;
    let mut data: Option<&[u8]> = None;

    // Each chunk: 4-byte id + 4-byte little-endian size + body, padded to a 2-byte boundary.
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]) as usize;
        let body_start = pos + 8;
        let body_end = body_start
            .checked_add(size)
            .ok_or(WavError::TruncatedData)?;
        if body_end > bytes.len() {
            return Err(WavError::TruncatedData);
        }
        let body = &bytes[body_start..body_end];

        if id == b"fmt " {
            if body.len() < 16 {
                return Err(WavError::MissingFmtChunk);
            }
            let fmt = u16::from_le_bytes([body[0], body[1]]);
            let ch = u16::from_le_bytes([body[2], body[3]]);
            let rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
            let bits = u16::from_le_bytes([body[14], body[15]]);
            if fmt != FMT_IEEE_FLOAT {
                return Err(WavError::UnsupportedFormat(fmt));
            }
            if bits != BITS_PER_SAMPLE {
                return Err(WavError::UnsupportedBits(bits));
            }
            spec = Some(WavSpec {
                sample_rate_hz: rate,
                channels: ch,
            });
        } else if id == b"data" {
            data = Some(body);
        }

        // Advance to the next chunk, skipping the pad byte after an odd-sized body.
        pos = body_end + (size & 1);
    }

    let spec = spec.ok_or(WavError::MissingFmtChunk)?;
    let data = data.ok_or(WavError::MissingDataChunk)?;
    let samples = data
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    Ok((samples, spec))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mono_48k() -> WavSpec {
        WavSpec {
            sample_rate_hz: 48_000,
            channels: 1,
        }
    }

    /// The header bytes are the canonical WAV layout — hand-checked field by field for a 2-sample
    /// mono 48 kHz float file (data = 2 × 4 = 8 bytes).
    #[test]
    fn header_is_the_canonical_layout() {
        let h = wav_header(mono_48k(), 8);
        assert_eq!(&h[0..4], b"RIFF");
        // RIFF size = 36 + 8 = 44.
        assert_eq!(&h[4..8], &44u32.to_le_bytes());
        assert_eq!(&h[8..12], b"WAVE");
        assert_eq!(&h[12..16], b"fmt ");
        assert_eq!(&h[16..20], &16u32.to_le_bytes());
        // Format tag 3 (IEEE float), 1 channel.
        assert_eq!(&h[20..22], &[3, 0]);
        assert_eq!(&h[22..24], &[1, 0]);
        // 48000 Hz = 0x0000_BB80 → LE bytes 80 BB 00 00.
        assert_eq!(&h[24..28], &[0x80, 0xBB, 0x00, 0x00]);
        // byte rate = 48000 × 1 × 4 = 192000 = 0x0002_EE00 → LE 00 EE 02 00.
        assert_eq!(&h[28..32], &[0x00, 0xEE, 0x02, 0x00]);
        // block align = 1 × 4 = 4; bits per sample = 32.
        assert_eq!(&h[32..34], &[4, 0]);
        assert_eq!(&h[34..36], &[32, 0]);
        assert_eq!(&h[36..40], b"data");
        assert_eq!(&h[40..44], &8u32.to_le_bytes());
    }

    /// A full-scale sample encodes to its known IEEE-754 bit pattern: 1.0 = 0x3F80_0000 → LE
    /// 00 00 80 3F, sitting right after the 44-byte header.
    #[test]
    fn full_scale_sample_has_the_known_float_bits() {
        let wav = encode_wav(&[1.0], mono_48k());
        assert_eq!(wav.len(), WAV_HEADER_LEN + 4);
        assert_eq!(&wav[WAV_HEADER_LEN..], &[0x00, 0x00, 0x80, 0x3F]);
    }

    /// Encode → decode is **bit-exact** for f32 (no quantization), including the signed-zero and
    /// rail edge cases, and the spec round-trips.
    #[test]
    fn round_trip_is_bit_exact() {
        let samples = [0.0_f32, 1.0, -1.0, 0.5, -0.5, 0.123_456_79, -0.0];
        let wav = encode_wav(&samples, mono_48k());
        let (back, spec) = decode_wav(&wav).expect("valid WAV");
        assert_eq!(spec, mono_48k());
        assert_eq!(back.len(), samples.len());
        for (a, b) in samples.iter().zip(&back) {
            // Compare raw bits so +0.0 vs −0.0 would be caught.
            assert_eq!(a.to_bits(), b.to_bits(), "sample {a} did not round-trip");
        }
    }

    /// An empty take is a valid, decodable zero-length WAV.
    #[test]
    fn empty_take_round_trips() {
        let wav = encode_wav(&[], mono_48k());
        let (back, spec) = decode_wav(&wav).expect("valid empty WAV");
        assert!(back.is_empty());
        assert_eq!(spec, mono_48k());
    }

    /// Decoding is total: garbage, wrong magic, and truncated chunks are errors, not panics.
    #[test]
    fn malformed_input_is_an_error_not_a_panic() {
        assert_eq!(decode_wav(&[]), Err(WavError::TooShort));
        assert_eq!(decode_wav(b"XXXXddddWAVE"), Err(WavError::BadRiffMagic));
        assert_eq!(decode_wav(b"RIFFddddXXXX"), Err(WavError::BadWaveMagic));

        // A well-formed header claiming 999 data bytes that aren't there → TruncatedData.
        let mut wav = encode_wav(&[1.0], mono_48k());
        wav[40..44].copy_from_slice(&999u32.to_le_bytes());
        assert_eq!(decode_wav(&wav), Err(WavError::TruncatedData));
    }

    /// Unknown chunks between `fmt ` and `data` are skipped, not fatal (a real writer may add e.g.
    /// a `LIST`/`fact` chunk). Insert a `fact` chunk and confirm the samples still decode.
    #[test]
    fn unknown_chunks_are_skipped() {
        let samples = [0.25_f32, -0.75];
        let base = encode_wav(&samples, mono_48k());
        // Splice a 4-byte "fact" chunk (id + size=4 + body) in front of the `data` chunk (at 36).
        let mut spliced = Vec::new();
        spliced.extend_from_slice(&base[..36]);
        spliced.extend_from_slice(b"fact");
        spliced.extend_from_slice(&4u32.to_le_bytes());
        spliced.extend_from_slice(&2u32.to_le_bytes()); // fact body (nominal sample count)
        spliced.extend_from_slice(&base[36..]);
        // Fix the RIFF size to account for the inserted 12 bytes.
        let new_riff = u32::from_le_bytes([spliced[4], spliced[5], spliced[6], spliced[7]]) + 12;
        spliced[4..8].copy_from_slice(&new_riff.to_le_bytes());

        let (back, _) = decode_wav(&spliced).expect("valid WAV with an extra chunk");
        assert_eq!(back, samples);
    }
}
