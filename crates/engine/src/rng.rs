//! Seeded, deterministic, splittable random number generation.
//!
//! Every stochastic source in the engine (device noise floors, hum phase, …) draws from
//! here, so a given seed reproduces a run exactly — useful for stable tests and
//! golden-file renders. We always seed explicitly: there is no ambient entropy
//! (`thread_rng`) and no system time, both for determinism and `wasm32` portability.
//!
//! Built on the `rand` ecosystem: the generator is [`rand_pcg::Pcg64Mcg`] (reproducible
//! and free of any `getrandom`/entropy backend), with uniform draws from `rand` and the
//! Gaussian from [`rand_distr::StandardNormal`]. [`Rng::split`] derives an independent
//! child stream so each device can own a reproducible stream uncorrelated with its
//! neighbours.

use rand::Rng as _;
use rand::{RngCore, SeedableRng};
use rand_distr::StandardNormal;
use rand_pcg::Pcg64Mcg;

/// A seeded, deterministic PRNG.
///
/// Construct with [`Rng::from_seed`]. It deliberately has no `Default` — an implicit seed
/// would hide nondeterminism, which is exactly what this type exists to prevent.
#[derive(Debug, Clone)]
pub struct Rng(Pcg64Mcg);

impl Rng {
    /// Create an RNG from a 64-bit seed.
    #[must_use]
    pub fn from_seed(seed: u64) -> Self {
        Self(Pcg64Mcg::seed_from_u64(seed))
    }

    /// Derive an independent child stream.
    ///
    /// Advances `self`, so repeated splits yield distinct children; given the same parent
    /// seed, the sequence of splits is identical run to run.
    #[must_use]
    pub fn split(&mut self) -> Self {
        Self::from_seed(self.0.next_u64())
    }

    /// Next raw 32-bit value.
    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }

    /// Next raw 64-bit value.
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }

    /// Uniform `f32` in `[0, 1)`.
    #[inline]
    pub fn next_f32_unit(&mut self) -> f32 {
        self.0.gen_range(0.0_f32..1.0)
    }

    /// Uniform `f32` in `[-1, 1)` — convenient for white noise.
    #[inline]
    pub fn next_f32_bipolar(&mut self) -> f32 {
        self.0.gen_range(-1.0_f32..1.0)
    }

    /// Standard-normal `f32` (mean 0, standard deviation 1).
    #[inline]
    pub fn next_gaussian(&mut self) -> f32 {
        self.0.sample::<f32, _>(StandardNormal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = Rng::from_seed(0xDEAD_BEEF);
        let mut b = Rng::from_seed(0xDEAD_BEEF);
        for _ in 0..16 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = Rng::from_seed(1);
        let mut b = Rng::from_seed(2);
        assert_ne!(a.next_u32(), b.next_u32());
    }

    #[test]
    fn split_is_reproducible() {
        let run = |seed| {
            let mut parent = Rng::from_seed(seed);
            let mut child = parent.split();
            (0..8).map(|_| child.next_u32()).collect::<Vec<_>>()
        };
        assert_eq!(run(7), run(7));
    }

    #[test]
    fn split_children_are_independent() {
        let mut parent = Rng::from_seed(99);
        let mut a = parent.split();
        let mut b = parent.split();
        let sa: Vec<u32> = (0..8).map(|_| a.next_u32()).collect();
        let sb: Vec<u32> = (0..8).map(|_| b.next_u32()).collect();
        assert_ne!(sa, sb);
    }

    #[test]
    fn uniform_unit_stays_in_range() {
        let mut rng = Rng::from_seed(123);
        for _ in 0..10_000 {
            assert!((0.0..1.0).contains(&rng.next_f32_unit()));
        }
    }

    #[test]
    fn uniform_bipolar_stays_in_range() {
        let mut rng = Rng::from_seed(123);
        for _ in 0..10_000 {
            assert!((-1.0..1.0).contains(&rng.next_f32_bipolar()));
        }
    }

    #[test]
    fn gaussian_has_expected_mean_and_variance() {
        // 100k draws from a fixed seed: a standard normal has mean 0, variance 1.
        // Margins are wide relative to the sampling error (~0.003), so this is a
        // deterministic check, not a flaky statistical one.
        let mut rng = Rng::from_seed(0x1234_5678);
        let n = 100_000;
        let mut sum = 0.0_f64;
        let mut sum_sq = 0.0_f64;
        for _ in 0..n {
            let x = f64::from(rng.next_gaussian());
            sum += x;
            sum_sq += x * x;
        }
        let mean = sum / f64::from(n);
        let variance = sum_sq / f64::from(n) - mean * mean;
        assert!(mean.abs() < 0.02, "mean = {mean}");
        assert!((variance - 1.0).abs() < 0.03, "variance = {variance}");
    }

    #[test]
    fn gaussian_is_reproducible() {
        let mut a = Rng::from_seed(55);
        let mut b = Rng::from_seed(55);
        for _ in 0..16 {
            // Compare bit patterns for exact equality without float `==`.
            assert_eq!(a.next_gaussian().to_bits(), b.next_gaussian().to_bits());
        }
    }
}
