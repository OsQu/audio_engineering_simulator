//! A digital converter's word length.

/// The bit depth (word length) of a digital-audio stream, e.g. 16 or 24 bits.
///
/// It fixes the quantization grid — step `Δ = FS / 2^(bits−1)` for a signed PCM word.
/// The samples themselves stay linear, normalized `f32`; the depth records the resolution they
/// were quantized at, so a downstream consumer knows the stream's noise floor.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BitDepth(u32);

impl BitDepth {
    /// Create a bit depth of `bits`.
    ///
    /// # Panics
    /// Panics unless `2 <= bits <= 32`: a signed PCM word needs a sign bit plus magnitude, and
    /// beyond 32 bits an `f32`-normalized sample can't faithfully address the grid anyway.
    /// Construction-time check, never on the hot path.
    #[must_use]
    pub fn new(bits: u32) -> Self {
        assert!(
            (2..=32).contains(&bits),
            "BitDepth must be in 2..=32, got {bits}"
        );
        Self(bits)
    }

    /// The word length in bits.
    pub fn get(self) -> u32 {
        self.0
    }

    /// The quantization step `Δ` for a peak full-scale of `full_scale`: `Δ = FS / 2^(bits−1)`.
    ///
    /// A signed `bits`-bit word spans `2^bits` codes over `[−FS, +FS)`, so adjacent codes are
    /// `2·FS / 2^bits = FS / 2^(bits−1)` apart. Computed in `f64`; the AD uses it to quantize
    /// and the tests use it as the noise-floor oracle (RMS `Δ/2` with TPDF dither).
    pub fn step(self, full_scale: f64) -> f64 {
        full_scale / 2.0_f64.powi(self.0 as i32 - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn get_round_trips() {
        assert_eq!(BitDepth::new(24).get(), 24);
    }

    #[test]
    fn step_matches_hand_calc() {
        // 16-bit over a ±1.0 full scale: Δ = 1 / 2^15 = 1 / 32768 = 3.0517578e-5.
        assert_relative_eq!(BitDepth::new(16).step(1.0), 1.0 / 32_768.0, epsilon = 1e-12);
        // 24-bit over ±1.0: Δ = 1 / 2^23.
        assert_relative_eq!(
            BitDepth::new(24).step(1.0),
            1.0 / 8_388_608.0,
            epsilon = 1e-15
        );
    }

    #[test]
    #[should_panic(expected = "must be in 2..=32")]
    fn rejects_one_bit() {
        let _ = BitDepth::new(1);
    }

    #[test]
    #[should_panic(expected = "must be in 2..=32")]
    fn rejects_too_deep() {
        let _ = BitDepth::new(33);
    }
}
