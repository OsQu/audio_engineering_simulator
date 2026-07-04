//! A routing matrix: an N-input × M-output digital crosspoint mixer, the runtime-switchable routing an
//! interface/mixer exposes (the 8i6's is Focusrite Control's mixer, simplified to per-crosspoint gains).

use super::Node;
use crate::param::{ParamDecl, ParamId, Params};
use crate::port::{AudioFormat, DigitalFace, InputPort, OutputPort};
use crate::signal::{BitDepth, Lane, SampleRate};

/// A **routing matrix**: `n` mono digital inputs × `m` mono digital outputs, with an `n·m` grid of
/// **crosspoint gains**. Output `j` is `Σ_i in_i · g[i][j]` — each output a gain-weighted sum of the
/// inputs, so the same node expresses a router (one unity crosspoint per output), a mixer (several), or
/// a mute (all zero). The gains are smoothed control params, so re-routing is **click-free and needs no
/// recompile** — the runtime-switchable-routing seam from `catalog.rs` (routing "lives inside a node
/// behind a control param"), as opposed to user-repatching, which is a graph edit.
///
/// This is a simplification of a real digital mixer (no per-output pan/solo/metering, just gains), the
/// "correct-enough, never false" line: the routing and level are real, the console features are not.
///
/// The crosspoint from input `i` to output `j` is param [`crosspoint(i, j)`](Self::crosspoint); all
/// ports share one `rate`/`bits`. Accumulation is `f64` (the summing-precision rule). `n` inputs; `m`
/// outputs — the second multi-output node (after the demux).
pub struct Matrix {
    m_out: usize,
    /// Construction-default crosspoint gains, row-major by input then output (`i·m + j`). Used as the
    /// per-param decl default and as the `process` fallback when run outside a schedule.
    defaults: Vec<f32>,
    param_decls: Vec<ParamDecl>,
    inputs: Vec<InputPort>,
    outputs: Vec<OutputPort>,
}

impl Matrix {
    /// Largest crosspoint gain (+12 dB of makeup) and the de-zipper glide, matching the other stages.
    const MAX_GAIN: f32 = 4.0;
    const SMOOTH_MS: f32 = 5.0;

    /// The [`ParamId`] of the crosspoint from input `i` to output `j` in a matrix `m_out` wide:
    /// `i·m_out + j` (row-major). The host drives routing through these.
    #[must_use]
    pub fn crosspoint(i: usize, j: usize, m_out: usize) -> ParamId {
        ParamId((i * m_out + j) as u32)
    }

    /// A matrix of `n_in` × `m_out` mono `rate`/`bits` channels, with `defaults` the initial crosspoint
    /// gains (row-major, `i·m_out + j`) — the routing it sits at until the host moves a crosspoint. A
    /// device authors identity-ish defaults here to reproduce its fixed routing.
    ///
    /// # Panics
    /// Panics unless `n_in ≥ 1`, `m_out ≥ 1`, and `defaults.len() == n_in · m_out`. Construction-time.
    #[must_use]
    pub fn new(
        rate: SampleRate,
        bits: BitDepth,
        n_in: usize,
        m_out: usize,
        defaults: Vec<f32>,
    ) -> Self {
        assert!(n_in >= 1 && m_out >= 1, "Matrix needs ≥1 input and output");
        assert!(
            defaults.len() == n_in * m_out,
            "Matrix defaults must have n_in·m_out = {} entries, got {}",
            n_in * m_out,
            defaults.len()
        );
        let mono = DigitalFace::new(AudioFormat::new(rate, bits, 1));
        let mut param_decls = Vec::with_capacity(n_in * m_out);
        for i in 0..n_in {
            for j in 0..m_out {
                param_decls.push(ParamDecl {
                    id: Self::crosspoint(i, j, m_out),
                    default: defaults[i * m_out + j],
                    min: 0.0,
                    max: Self::MAX_GAIN,
                    smooth_ms: Self::SMOOTH_MS,
                });
            }
        }
        Self {
            m_out,
            defaults,
            param_decls,
            inputs: (0..n_in).map(|_| mono.into()).collect(),
            outputs: (0..m_out).map(|_| mono.into()).collect(),
        }
    }
}

impl Node for Matrix {
    fn inputs(&self) -> &[InputPort] {
        &self.inputs
    }

    fn outputs(&self) -> &[OutputPort] {
        &self.outputs
    }

    fn params(&self) -> &[ParamDecl] {
        &self.param_decls
    }

    fn process(&mut self, params: &Params, inputs: &[Lane], outputs: &mut [Lane]) {
        // Output j = Σ_i in_i · g[i][j], accumulated in f64. Gains are read per sample so a routing
        // change de-zippers across the block. No alloc, no panic (indexing avoided via iterators /
        // `get`), no locks.
        let m = self.m_out;
        for (j, out_lane) in outputs.iter_mut().enumerate() {
            let out = out_lane.sample_mut().as_mut_slice();
            for (s, o) in out.iter_mut().enumerate() {
                let mut acc = 0.0_f64;
                for (i, in_lane) in inputs.iter().enumerate() {
                    let x = in_lane.sample().as_slice().get(s).copied().unwrap_or(0.0);
                    let fallback = self.defaults.get(i * m + j).copied().unwrap_or(0.0);
                    let g = params.value_at_or(Self::crosspoint(i, j, m), s, fallback);
                    acc += f64::from(x) * f64::from(g);
                }
                *o = acc as f32;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::param::{Params, Smoother};
    use crate::signal::{ClockDomainId, Domain, SampleBuffer};

    fn fs() -> SampleRate {
        SampleRate::new(48_000.0)
    }
    fn bits() -> BitDepth {
        BitDepth::new(16)
    }

    /// Distinct constant per input channel, `len` samples each, as digital lanes.
    fn ins(vals: &[f32], len: usize) -> Vec<Lane> {
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

    fn outs(m: usize, len: usize) -> Vec<Lane> {
        (0..m)
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

    #[test]
    fn declares_n_ins_m_outs_and_nm_params() {
        let mx = Matrix::new(fs(), bits(), 3, 2, vec![0.0; 6]);
        assert_eq!(mx.inputs().len(), 3);
        assert_eq!(mx.outputs().len(), 2);
        assert_eq!(mx.params().len(), 6);
        for p in mx.inputs() {
            assert_eq!(p.domain(), Domain::DigitalAudio);
        }
    }

    #[test]
    fn identity_defaults_pass_each_input_to_its_output() {
        // A 2×2 identity (crosspoints (0,0)=(1,1)=1, off-diagonal 0): out0=in0, out1=in1. Run outside a
        // schedule so the decl defaults are the fallback.
        let mut mx = Matrix::new(fs(), bits(), 2, 2, vec![1.0, 0.0, 0.0, 1.0]);
        let input = ins(&[0.3, -0.6], 8);
        let mut out = outs(2, 8);
        mx.process(&Params::EMPTY, &input, &mut out);
        assert!(out[0].sample().as_slice().iter().all(|&s| s == 0.3));
        assert!(out[1].sample().as_slice().iter().all(|&s| s == -0.6));
    }

    #[test]
    fn an_output_sums_the_inputs_routed_to_it() {
        // out0 = in0 + in1 (both crosspoints to output 0 at unity); a single output, two inputs.
        let mut mx = Matrix::new(fs(), bits(), 2, 1, vec![1.0, 1.0]);
        let input = ins(&[0.25, 0.5], 8);
        let mut out = outs(1, 8);
        mx.process(&Params::EMPTY, &input, &mut out);
        assert!(
            out[0]
                .sample()
                .as_slice()
                .iter()
                .all(|&s| (s - 0.75).abs() < 1e-6)
        );
    }

    #[test]
    fn a_settled_crosspoint_gain_scales_and_routes() {
        // 2×2: route in0 → out1 at 0.5, everything else off. Settled smoothers stand in for a schedule.
        let mut mx = Matrix::new(fs(), bits(), 2, 2, vec![0.0; 4]);
        let input = ins(&[0.8, 0.4], 8);
        let mut out = outs(2, 8);
        // Crosspoints in id order (0,0),(0,1),(1,0),(1,1): set (0,1) = 0.5.
        let smoothers = [
            Smoother::new(0.0, 0.0, Matrix::MAX_GAIN, 1.0),
            Smoother::new(0.5, 0.0, Matrix::MAX_GAIN, 1.0),
            Smoother::new(0.0, 0.0, Matrix::MAX_GAIN, 1.0),
            Smoother::new(0.0, 0.0, Matrix::MAX_GAIN, 1.0),
        ];
        mx.process(&Params::new(&smoothers), &input, &mut out);
        assert!(
            out[0].sample().as_slice().iter().all(|&s| s == 0.0),
            "out0 unrouted"
        );
        assert!(
            out[1]
                .sample()
                .as_slice()
                .iter()
                .all(|&s| (s - 0.4).abs() < 1e-6),
            "out1 = in0 · 0.5 = 0.4"
        );
    }

    #[test]
    #[should_panic(expected = "n_in·m_out")]
    fn rejects_wrong_default_length() {
        let _ = Matrix::new(fs(), bits(), 2, 2, vec![1.0, 0.0]); // needs 4
    }
}
