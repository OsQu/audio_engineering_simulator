//! WAV output for offline renders.
//!
//! The canonical render format is **32-bit float, mono**. Float is deliberate: a PCM-integer
//! writer would quantize the (transparent) captured samples and add its *own* quantization noise,
//! which would contaminate the measurement of the noise produced by the *modeled* AD at low bit
//! depth. Float32 stores the capture's output losslessly, so the only artifacts in a render are
//! the ones we're studying. (Mono: the engine's converters/lanes are mono; multichannel isn't
//! modeled yet.)

use std::fs::File;
use std::io::{Seek, Write};
use std::path::Path;

use engine::SampleRate;
use hound::{SampleFormat, WavSpec, WavWriter};

/// The mono float32 spec at `rate`.
fn spec(rate: SampleRate) -> WavSpec {
    WavSpec {
        channels: 1,
        sample_rate: rate.as_hz() as u32,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    }
}

/// Write `samples` (mono, normalized ±1.0) to any seekable sink as a 32-bit float WAV at `rate`.
///
/// Generic over the sink so renders go to a file and tests round-trip through an in-memory
/// [`Cursor`](std::io::Cursor) — no temp files.
///
/// # Errors
/// Propagates any [`hound::Error`] from writing or finalizing the stream.
pub fn write_mono_f32_to<W: Write + Seek>(
    sink: W,
    samples: &[f32],
    rate: SampleRate,
) -> hound::Result<()> {
    let mut writer = WavWriter::new(sink, spec(rate))?;
    for &s in samples {
        writer.write_sample(s)?;
    }
    writer.finalize()
}

/// Write `samples` (mono, normalized ±1.0) to a 32-bit float WAV file at `path`, sampled at `rate`.
///
/// # Errors
/// Propagates an I/O error opening `path`, or any [`hound::Error`] while writing.
pub fn write_mono_f32(
    path: impl AsRef<Path>,
    samples: &[f32],
    rate: SampleRate,
) -> hound::Result<()> {
    write_mono_f32_to(File::create(path)?, samples, rate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::WavReader;
    use std::io::Cursor;

    #[test]
    fn round_trips_mono_f32_bit_exact() {
        let rate = SampleRate::new(48_000.0);
        let samples = [0.0_f32, 0.5, -0.5, 1.0, -1.0, 0.123_456_79];

        // Write to an in-memory buffer, then read it back — float32 is exact, so the round trip
        // must be bit-for-bit.
        let mut buf = Cursor::new(Vec::new());
        write_mono_f32_to(&mut buf, &samples, rate).unwrap();
        buf.set_position(0);

        let reader = WavReader::new(buf).unwrap();
        let spec = reader.spec();
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.sample_rate, 48_000);
        assert_eq!(spec.bits_per_sample, 32);
        assert_eq!(spec.sample_format, SampleFormat::Float);

        let read: Vec<f32> = reader.into_samples::<f32>().map(Result::unwrap).collect();
        assert_eq!(read, samples);
    }
}
