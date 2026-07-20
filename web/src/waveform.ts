// Host-side waveform reduction for the DAW's take display (Story 5.11.6). A take's raw PCM (the frames
// the storage worker returns, header already stripped) is reduced to a small array of per-bucket peak
// magnitudes the mixer draws as a static thumbnail. **Display only** — this is a filesystem read the
// host renders, not the host doing audio (the sim still owns encode/decode + all playback).
//
// Our take files are little-endian `f32` PCM, so the host can view them directly; a foreign WAV would
// go through the sim's `decode_wav` instead, but we only ever draw our own takes here.

/** View a take's raw PCM bytes (little-endian `f32` frames) as samples. Copies to a fresh, 4-aligned
 *  buffer so the `Float32Array` view is always valid regardless of the source's byte offset. */
export function pcmToFloat32(pcm: Uint8Array): Float32Array {
  const copy = pcm.slice(); // standalone buffer at offset 0
  return new Float32Array(copy.buffer, 0, copy.byteLength >> 2); // >>2 = ÷4 (drop any partial frame)
}

/** Reduce `samples` to at most `buckets` per-bucket **peak magnitudes** (max |sample|, 0..~1) for a
 *  waveform thumbnail. Fewer samples than buckets ⇒ one bucket per sample; empty input ⇒ `[]`. Pure. */
export function downsamplePeaks(samples: Float32Array, buckets: number): number[] {
  const n = samples.length;
  if (n === 0 || buckets <= 0) return [];
  const count = Math.min(buckets, n);
  const peaks = new Array<number>(count);
  for (let i = 0; i < count; i++) {
    const start = Math.floor((i * n) / count);
    const end = Math.max(start + 1, Math.floor(((i + 1) * n) / count));
    let peak = 0;
    for (let j = start; j < end && j < n; j++) {
      const a = Math.abs(samples[j]);
      if (a > peak) peak = a;
    }
    peaks[i] = peak;
  }
  return peaks;
}

/** Convenience: raw take PCM → `buckets` peak magnitudes, for the mixer's per-track thumbnail. */
export function peaksFromPcm(pcm: Uint8Array, buckets: number): number[] {
  return downsamplePeaks(pcmToFloat32(pcm), buckets);
}
