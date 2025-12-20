//! Hub Configuration - Thread count and runtime settings
//!
//! This module provides configuration for the Hub's execution model,
//! particularly the number of worker cores for parallel execution.
//!
//! The `core_count` setting is stored for when parallel execution is
//! implemented. Currently, the Hub uses the same execution path
//! regardless of core count.

use serde::{Deserialize, Serialize};

/// Configuration for Hub execution
///
/// Controls the number of worker cores for parallel execution.
///
/// # Example
///
/// ```
/// use pulsive_hub::HubConfig;
///
/// // Single-core mode (default)
/// let config = HubConfig::default();
/// assert!(config.is_single_core());
///
/// // Configure for 4 cores (clamped to available cores)
/// let config = HubConfig::with_core_count(4);
/// assert_eq!(config.core_count(), 4.min(pulsive_hub::max_cores()));
/// // Not single-core if we have at least 2 cores available
/// if pulsive_hub::max_cores() >= 2 {
///     assert!(!config.is_single_core());
/// }
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
}

impl HubConfig {
    /// Create a new configuration with the specified core count
    ///
    /// The core count is clamped to `[1, max_cores()]`.
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
    /// ```
    pub fn with_core_count(core_count: usize) -> Self {
        Self {
            core_count: core_count.clamp(1, max_cores()),
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
}

impl Default for HubConfig {
    /// Create a default configuration with single-core mode
    fn default() -> Self {
        Self { core_count: 1 }
    }
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
}
