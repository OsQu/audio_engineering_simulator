import { describe, expect, it } from "vitest";
import { downsamplePeaks, pcmToFloat32, peaksFromPcm } from "../src/waveform";

function pcm(...values: number[]): Uint8Array {
  return new Uint8Array(new Float32Array(values).buffer.slice(0));
}

describe("waveform", () => {
  it("views raw f32 PCM as samples", () => {
    const samples = pcmToFloat32(pcm(0.25, -0.5, 1));
    expect(Array.from(samples)).toEqual([0.25, -0.5, 1].map((v) => Math.fround(v)));
  });

  it("views PCM at a non-zero byte offset (realigned)", () => {
    // A subarray with a byteOffset not divisible by 4 must still read correctly.
    const backing = new Uint8Array(1 + 4 * 2);
    new DataView(backing.buffer).setFloat32(1, 0.5, true);
    new DataView(backing.buffer).setFloat32(5, -0.5, true);
    const view = backing.subarray(1); // byteOffset 1 — unaligned for Float32Array
    expect(Array.from(pcmToFloat32(view))).toEqual([0.5, -0.5]);
  });

  it("drops a trailing partial frame rather than throwing", () => {
    const bytes = new Uint8Array(6); // 1 full f32 + 2 stray bytes
    new DataView(bytes.buffer).setFloat32(0, 0.75, true);
    expect(Array.from(pcmToFloat32(bytes))).toEqual([0.75]);
  });

  it("reduces to per-bucket peak magnitudes", () => {
    const samples = new Float32Array([0.1, -0.9, 0.3, 0.2, -0.4, 0.8]);
    // 3 buckets of 2 samples each → max abs of each pair.
    expect(downsamplePeaks(samples, 3)).toEqual([
      Math.fround(0.9),
      Math.fround(0.3),
      Math.fround(0.8),
    ]);
  });

  it("uses one bucket per sample when asked for more buckets than samples", () => {
    const samples = new Float32Array([0.2, -0.6]);
    expect(downsamplePeaks(samples, 100)).toEqual([Math.fround(0.2), Math.fround(0.6)]);
  });

  it("is empty for empty input or non-positive buckets", () => {
    expect(downsamplePeaks(new Float32Array(0), 10)).toEqual([]);
    expect(downsamplePeaks(new Float32Array([1]), 0)).toEqual([]);
    expect(peaksFromPcm(new Uint8Array(0), 10)).toEqual([]);
  });

  it("peaksFromPcm reduces a take's raw PCM to a thumbnail", () => {
    const peaks = peaksFromPcm(pcm(0.1, -0.2, 0.9, 0.05), 2);
    expect(peaks).toEqual([Math.fround(0.2), Math.fround(0.9)]);
  });
});
