//! Deterministic random number generator
//!
//! Uses a simple xorshift64 algorithm for reproducibility across platforms.
//! This ensures the same seed produces the same sequence on all clients.

use serde::{Deserialize, Serialize};

/// A deterministic random number generator
///
/// Uses xorshift64 for simplicity and reproducibility.
/// Never use std::random or other non-deterministic sources in game logic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameRng {
    state: u64,
}

impl GameRng {
    /// Create a new RNG with the given seed
    pub fn new(seed: u64) -> Self {
        // Ensure non-zero state (xorshift requires this)
        let state = if seed == 0 { 1 } else { seed };
        Self { state }
    }

    /// Create an RNG from a saved state
    pub fn from_state(state: u64) -> Self {
        let state = if state == 0 { 1 } else { state };
        Self { state }
    }

    /// Get the current state (useful for saving/loading)
    pub fn state(&self) -> u64 {
        self.state
    }

    /// Generate the next raw u64 value
    pub fn next_u64(&mut self) -> u64 {
        // xorshift64 algorithm
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    /// Generate a random u32
    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// Generate a random f64 in range [0, 1)
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() as f64) / (u64::MAX as f64 + 1.0)
    }

    /// Generate a random f64 in range [min, max)
    pub fn range_f64(&mut self, min: f64, max: f64) -> f64 {
        min + self.next_f64() * (max - min)
    }

    /// Generate a random i64 in range [min, max]
    pub fn range_i64(&mut self, min: i64, max: i64) -> i64 {
        let range = (max - min + 1) as u64;
        let value = self.next_u64() % range;
        min + value as i64
    }

    /// Generate a random bool with given probability of true
    pub fn chance(&mut self, probability: f64) -> bool {
        self.next_f64() < probability
    }

    /// Generate a random bool (50% chance)
    pub fn coin_flip(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }

    /// Pick a random index for a weighted list
    /// Returns None if weights is empty or all weights are zero
    pub fn weighted_index(&mut self, weights: &[f64]) -> Option<usize> {
        let total: f64 = weights.iter().sum();
        if total <= 0.0 || weights.is_empty() {
            return None;
        }

        let mut threshold = self.next_f64() * total;
        for (i, &weight) in weights.iter().enumerate() {
            threshold -= weight;
            if threshold <= 0.0 {
                return Some(i);
            }
        }

        // Fallback to last element (shouldn't happen with proper floats)
        Some(weights.len() - 1)
    }

    /// Shuffle a slice in place (Fisher-Yates)
    pub fn shuffle<T>(&mut self, slice: &mut [T]) {
        for i in (1..slice.len()).rev() {
            let j = (self.next_u64() as usize) % (i + 1);
            slice.swap(i, j);
        }
    }

    /// Pick a random element from a slice
    pub fn pick<'a, T>(&mut self, slice: &'a [T]) -> Option<&'a T> {
        if slice.is_empty() {
            None
        } else {
            let i = (self.next_u64() as usize) % slice.len();
            Some(&slice[i])
        }
    }
}

impl Default for GameRng {
    fn default() -> Self {
        Self::new(12345)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_determinism() {
        let mut rng1 = GameRng::new(42);
        let mut rng2 = GameRng::new(42);

        for _ in 0..100 {
            assert_eq!(rng1.next_u64(), rng2.next_u64());
        }
    }

    #[test]
    fn test_range() {
        let mut rng = GameRng::new(42);

        for _ in 0..100 {
            let f = rng.next_f64();
            assert!(f >= 0.0 && f < 1.0);
        }

        for _ in 0..100 {
            let i = rng.range_i64(10, 20);
            assert!(i >= 10 && i <= 20);
        }
    }

    #[test]
    fn test_weighted_index() {
        let mut rng = GameRng::new(42);
        let weights = [1.0, 2.0, 3.0]; // 1/6, 2/6, 3/6 probability

        let mut counts = [0; 3];
        for _ in 0..6000 {
            if let Some(i) = rng.weighted_index(&weights) {
                counts[i] += 1;
            }
        }

        // Rough check that weighting works (index 2 should have ~3x index 0)
        assert!(counts[2] > counts[0] * 2);
    }

    #[test]
    fn test_shuffle() {
        let mut rng = GameRng::new(42);
        let original = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let mut shuffled = original.clone();
        rng.shuffle(&mut shuffled);

        // Should still contain same elements
        let mut sorted = shuffled.clone();
        sorted.sort();
        assert_eq!(sorted, original);

        // Should be different order (very unlikely to be same with 10 elements)
        assert_ne!(shuffled, original);
    }
}
