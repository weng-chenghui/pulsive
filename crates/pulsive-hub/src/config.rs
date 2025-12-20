//! Hub Configuration - Thread count and runtime settings
//!
//! This module provides configuration for the Hub's execution model,
//! including core count and global seed for deterministic execution.
//!
//! # Deterministic RNG
//!
//! The `global_seed` is used to derive deterministic per-core RNG seeds.
//! Each core gets a unique seed derived from:
//! - `global_seed`: The hub's master seed
//! - `core_id`: The core's identifier within a group
//! - `tick`: The current simulation tick
//!
//! This ensures:
//! - Same seed + same inputs = same outputs
//! - RNG streams are independent between cores
//! - Replay produces identical results
//! - Works with any number of cores

use pulsive_core::Rng;
use serde::{Deserialize, Serialize};

/// Configuration for Hub execution
///
/// Controls the number of worker cores and the global seed for
/// deterministic parallel execution.
///
/// # Example
///
/// ```
/// use pulsive_hub::HubConfig;
///
/// // Single-core mode with default seed
/// let config = HubConfig::default();
/// assert!(config.is_single_core());
/// assert_eq!(config.global_seed(), 12345); // Default seed
///
/// // Configure for 4 cores with custom seed
/// let config = HubConfig::new(4, 42);
/// assert_eq!(config.core_count(), 4.min(pulsive_hub::max_cores()));
/// assert_eq!(config.global_seed(), 42);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubConfig {
    /// Number of worker cores for parallel execution
    ///
    /// - `1`: Single-core mode (default)
    /// - `> 1`: Multiple cores for future parallel execution
    ///
    /// This value is clamped to `[1, max_cores()]`.
    core_count: usize,

    /// Global seed for deterministic RNG
    ///
    /// This seed is used to derive per-core RNG seeds using:
    /// `hash(global_seed, core_id, tick)`
    ///
    /// This ensures each core has an independent, deterministic RNG stream.
    global_seed: u64,
}

/// Default global seed for deterministic RNG
pub const DEFAULT_GLOBAL_SEED: u64 = 12345;

impl HubConfig {
    /// Create a new configuration with the specified core count and seed
    ///
    /// The core count is clamped to `[1, max_cores()]`.
    ///
    /// # Arguments
    ///
    /// * `core_count` - Number of worker cores (1 for serial, >1 for parallel)
    /// * `global_seed` - Master seed for deterministic per-core RNG
    ///
    /// # Example
    ///
    /// ```
    /// use pulsive_hub::HubConfig;
    ///
    /// let config = HubConfig::new(4, 42);
    /// assert_eq!(config.core_count(), 4.min(pulsive_hub::max_cores()));
    /// assert_eq!(config.global_seed(), 42);
    /// ```
    pub fn new(core_count: usize, global_seed: u64) -> Self {
        Self {
            core_count: core_count.clamp(1, max_cores()),
            global_seed,
        }
    }

    /// Create a new configuration with the specified core count
    ///
    /// The core count is clamped to `[1, max_cores()]`.
    /// Uses the default global seed.
    ///
    /// # Arguments
    ///
    /// * `core_count` - Number of worker cores (1 for serial, >1 for parallel)
    ///
    /// # Example
    ///
    /// ```
    /// use pulsive_hub::HubConfig;
    ///
    /// let config = HubConfig::with_core_count(4);
    /// assert_eq!(config.core_count(), 4.min(pulsive_hub::max_cores()));
    /// assert_eq!(config.global_seed(), pulsive_hub::DEFAULT_GLOBAL_SEED);
    /// ```
    pub fn with_core_count(core_count: usize) -> Self {
        Self {
            core_count: core_count.clamp(1, max_cores()),
            global_seed: DEFAULT_GLOBAL_SEED,
        }
    }

    /// Create a new configuration with the specified global seed
    ///
    /// Uses single-core mode.
    ///
    /// # Arguments
    ///
    /// * `global_seed` - Master seed for deterministic per-core RNG
    ///
    /// # Example
    ///
    /// ```
    /// use pulsive_hub::HubConfig;
    ///
    /// let config = HubConfig::with_seed(42);
    /// assert!(config.is_single_core());
    /// assert_eq!(config.global_seed(), 42);
    /// ```
    pub fn with_seed(global_seed: u64) -> Self {
        Self {
            core_count: 1,
            global_seed,
        }
    }

    /// Get the current core count
    ///
    /// Returns the number of worker cores configured for parallel execution.
    pub fn core_count(&self) -> usize {
        self.core_count
    }

    /// Set the number of worker cores
    ///
    /// The value is clamped to `[1, max_cores()]`.
    ///
    /// # Arguments
    ///
    /// * `n` - Number of worker cores
    ///
    /// # Example
    ///
    /// ```
    /// use pulsive_hub::HubConfig;
    ///
    /// let mut config = HubConfig::default();
    /// config.set_core_count(4);
    /// assert_eq!(config.core_count(), 4.min(pulsive_hub::max_cores()));
    /// ```
    pub fn set_core_count(&mut self, n: usize) {
        self.core_count = n.clamp(1, max_cores());
    }

    /// Get the global seed
    ///
    /// Returns the master seed used for deriving per-core RNG seeds.
    pub fn global_seed(&self) -> u64 {
        self.global_seed
    }

    /// Set the global seed
    ///
    /// # Arguments
    ///
    /// * `seed` - Master seed for deterministic per-core RNG
    ///
    /// # Example
    ///
    /// ```
    /// use pulsive_hub::HubConfig;
    ///
    /// let mut config = HubConfig::default();
    /// config.set_global_seed(42);
    /// assert_eq!(config.global_seed(), 42);
    /// ```
    pub fn set_global_seed(&mut self, seed: u64) {
        self.global_seed = seed;
    }

    /// Check if configured for single-core mode
    ///
    /// Returns true when `core_count == 1`.
    ///
    /// # Example
    ///
    /// ```
    /// use pulsive_hub::HubConfig;
    ///
    /// let config = HubConfig::default();
    /// assert!(config.is_single_core());
    ///
    /// let config = HubConfig::with_core_count(2);
    /// assert!(!config.is_single_core());
    /// ```
    pub fn is_single_core(&self) -> bool {
        self.core_count == 1
    }

    /// Create a deterministic RNG for a specific core at a specific tick
    ///
    /// This combines the global seed with the core ID and tick to produce
    /// a unique, deterministic RNG for each core at each tick.
    ///
    /// # Formula
    ///
    /// `seed = hash(global_seed, core_id, tick)`
    ///
    /// # Arguments
    ///
    /// * `core_id` - The core's identifier within a group
    /// * `tick` - The current simulation tick
    ///
    /// # Example
    ///
    /// ```
    /// use pulsive_hub::HubConfig;
    ///
    /// let config = HubConfig::with_seed(42);
    ///
    /// // Same inputs produce same RNG
    /// let mut rng1 = config.create_core_rng(0, 5);
    /// let mut rng2 = config.create_core_rng(0, 5);
    /// assert_eq!(rng1.next_u64(), rng2.next_u64());
    ///
    /// // Different cores get different RNG streams
    /// let mut rng_core0 = config.create_core_rng(0, 5);
    /// let mut rng_core1 = config.create_core_rng(1, 5);
    /// assert_ne!(rng_core0.next_u64(), rng_core1.next_u64());
    ///
    /// // Different ticks get different RNG streams
    /// let mut rng_tick5 = config.create_core_rng(0, 5);
    /// let mut rng_tick6 = config.create_core_rng(0, 6);
    /// assert_ne!(rng_tick5.next_u64(), rng_tick6.next_u64());
    /// ```
    pub fn create_core_rng(&self, core_id: usize, tick: u64) -> Rng {
        let seed = hash_seed(self.global_seed, core_id as u64, tick);
        Rng::new(seed)
    }
}

impl Default for HubConfig {
    /// Create a default configuration with single-core mode and default seed
    fn default() -> Self {
        Self {
            core_count: 1,
            global_seed: DEFAULT_GLOBAL_SEED,
        }
    }
}

/// Hash function for deterministic RNG seeding
///
/// Combines base_seed, core_id, and tick to produce unique per-core-per-tick seeds.
///
/// This uses a simple but effective mixing function that ensures:
/// - Same inputs always produce the same output (deterministic)
/// - Different inputs produce different outputs (good distribution)
/// - Changes to any input affect the output (avalanche effect)
pub fn hash_seed(base_seed: u64, core_id: u64, tick: u64) -> u64 {
    // Simple but effective mixing function
    let mut h = base_seed;
    h = h.wrapping_mul(0x517cc1b727220a95);
    h ^= core_id;
    h = h.wrapping_mul(0x517cc1b727220a95);
    h ^= tick;
    h = h.wrapping_mul(0x517cc1b727220a95);
    h
}

/// Get the maximum available cores on this system
///
/// This uses the `num_cpus` crate to detect the number of logical CPUs.
///
/// # Example
///
/// ```
/// use pulsive_hub::max_cores;
///
/// let cores = max_cores();
/// assert!(cores >= 1);
/// println!("This system has {} cores available", cores);
/// ```
pub fn max_cores() -> usize {
    num_cpus::get()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Core Count Tests
    // ========================================================================

    #[test]
    fn test_default_is_single_core() {
        let config = HubConfig::default();
        assert!(config.is_single_core());
        assert_eq!(config.core_count(), 1);
    }

    #[test]
    fn test_with_core_count() {
        let config = HubConfig::with_core_count(4);
        // Core count is clamped to max_cores, so we check it's at least what we expect
        // or clamped if the machine has fewer cores
        assert_eq!(config.core_count(), 4.min(max_cores()));
    }

    #[test]
    fn test_set_core_count() {
        let mut config = HubConfig::default();
        assert!(config.is_single_core());

        config.set_core_count(4);
        let expected = 4.min(max_cores());
        assert_eq!(config.core_count(), expected);
        assert_eq!(config.is_single_core(), expected == 1);

        // Set back to single core
        config.set_core_count(1);
        assert!(config.is_single_core());
    }

    #[test]
    fn test_core_count_clamped_minimum() {
        // 0 should be clamped to 1
        let config = HubConfig::with_core_count(0);
        assert_eq!(config.core_count(), 1);
        assert!(config.is_single_core());
    }

    #[test]
    fn test_core_count_clamped_maximum() {
        // Very high value should be clamped to max_cores
        let config = HubConfig::with_core_count(10000);
        assert_eq!(config.core_count(), max_cores());
    }

    #[test]
    fn test_max_cores() {
        let cores = max_cores();
        assert!(cores >= 1, "max_cores should be at least 1");
    }

    #[test]
    fn test_can_change_between_ticks() {
        let mut config = HubConfig::default();

        // Start single-core
        assert!(config.is_single_core());

        // Switch to parallel
        config.set_core_count(2);
        assert!(!config.is_single_core() || max_cores() == 1);

        // Switch back to single-core
        config.set_core_count(1);
        assert!(config.is_single_core());
    }

    // ========================================================================
    // Global Seed Tests
    // ========================================================================

    #[test]
    fn test_default_global_seed() {
        let config = HubConfig::default();
        assert_eq!(config.global_seed(), DEFAULT_GLOBAL_SEED);
    }

    #[test]
    fn test_with_seed() {
        let config = HubConfig::with_seed(42);
        assert!(config.is_single_core());
        assert_eq!(config.global_seed(), 42);
    }

    #[test]
    fn test_new_with_core_count_and_seed() {
        let config = HubConfig::new(4, 42);
        assert_eq!(config.core_count(), 4.min(max_cores()));
        assert_eq!(config.global_seed(), 42);
    }

    #[test]
    fn test_set_global_seed() {
        let mut config = HubConfig::default();
        assert_eq!(config.global_seed(), DEFAULT_GLOBAL_SEED);

        config.set_global_seed(99);
        assert_eq!(config.global_seed(), 99);
    }

    #[test]
    fn test_with_core_count_uses_default_seed() {
        let config = HubConfig::with_core_count(4);
        assert_eq!(config.global_seed(), DEFAULT_GLOBAL_SEED);
    }

    // ========================================================================
    // Hash Seed Tests
    // ========================================================================

    #[test]
    fn test_hash_seed_deterministic() {
        // Same inputs should always produce same output
        let seed1 = hash_seed(100, 0, 5);
        let seed2 = hash_seed(100, 0, 5);
        assert_eq!(seed1, seed2);
    }

    #[test]
    fn test_hash_seed_different_base_seeds() {
        let seed1 = hash_seed(100, 0, 5);
        let seed2 = hash_seed(101, 0, 5);
        assert_ne!(seed1, seed2);
    }

    #[test]
    fn test_hash_seed_different_core_ids() {
        let seed1 = hash_seed(100, 0, 5);
        let seed2 = hash_seed(100, 1, 5);
        assert_ne!(seed1, seed2);
    }

    #[test]
    fn test_hash_seed_different_ticks() {
        let seed1 = hash_seed(100, 0, 5);
        let seed2 = hash_seed(100, 0, 6);
        assert_ne!(seed1, seed2);
    }

    #[test]
    fn test_hash_seed_all_different() {
        // Ensure good distribution by checking many combinations
        let mut seeds = std::collections::HashSet::new();

        for base in [0, 1, 100, u64::MAX] {
            for core in 0..4 {
                for tick in 0..10 {
                    let seed = hash_seed(base, core, tick);
                    seeds.insert(seed);
                }
            }
        }

        // All 4 * 4 * 10 = 160 combinations should be unique
        assert_eq!(seeds.len(), 4 * 4 * 10);
    }

    // ========================================================================
    // Create Core RNG Tests
    // ========================================================================

    #[test]
    fn test_create_core_rng_deterministic() {
        let config = HubConfig::with_seed(42);

        // Same inputs produce same RNG sequence
        let mut rng1 = config.create_core_rng(0, 5);
        let mut rng2 = config.create_core_rng(0, 5);

        assert_eq!(rng1.next_u64(), rng2.next_u64());
        assert_eq!(rng1.next_u64(), rng2.next_u64());
        assert_eq!(rng1.next_u64(), rng2.next_u64());
    }

    #[test]
    fn test_create_core_rng_independent_cores() {
        let config = HubConfig::with_seed(42);

        // Different cores get different RNG streams
        let mut rng_core0 = config.create_core_rng(0, 5);
        let mut rng_core1 = config.create_core_rng(1, 5);

        assert_ne!(rng_core0.next_u64(), rng_core1.next_u64());
    }

    #[test]
    fn test_create_core_rng_independent_ticks() {
        let config = HubConfig::with_seed(42);

        // Different ticks get different RNG streams
        let mut rng_tick5 = config.create_core_rng(0, 5);
        let mut rng_tick6 = config.create_core_rng(0, 6);

        assert_ne!(rng_tick5.next_u64(), rng_tick6.next_u64());
    }

    #[test]
    fn test_create_core_rng_independent_seeds() {
        let config1 = HubConfig::with_seed(42);
        let config2 = HubConfig::with_seed(43);

        // Different global seeds produce different RNG streams
        let mut rng1 = config1.create_core_rng(0, 5);
        let mut rng2 = config2.create_core_rng(0, 5);

        assert_ne!(rng1.next_u64(), rng2.next_u64());
    }

    #[test]
    fn test_create_core_rng_many_cores() {
        let config = HubConfig::with_seed(12345);

        // Verify all cores get unique RNG streams
        let values: Vec<u64> = (0..100)
            .map(|core| {
                let mut rng = config.create_core_rng(core, 0);
                rng.next_u64()
            })
            .collect();

        // All values should be unique
        let unique: std::collections::HashSet<_> = values.iter().collect();
        assert_eq!(unique.len(), 100);
    }

    #[test]
    fn test_create_core_rng_many_ticks() {
        let config = HubConfig::with_seed(12345);

        // Verify all ticks get unique RNG streams for the same core
        let values: Vec<u64> = (0..100)
            .map(|tick| {
                let mut rng = config.create_core_rng(0, tick);
                rng.next_u64()
            })
            .collect();

        // All values should be unique
        let unique: std::collections::HashSet<_> = values.iter().collect();
        assert_eq!(unique.len(), 100);
    }
}
