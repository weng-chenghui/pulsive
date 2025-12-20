//! Core - Thin wrapper around pulsive-core's Runtime + Model
//!
//! This is a **convenience wrapper only**. It bundles:
//! - `pulsive_core::Runtime` (unchanged, used as-is)
//! - `pulsive_core::Model` (unchanged, used as-is)
//! - A `CoreId` for identification within a group
//! - An RNG seed for deterministic parallel execution
//!
//! pulsive-core does NOT know about pulsive-hub. This wrapper simply
//! provides a convenient way for CoreGroup to manage multiple Runtime+Model
//! pairs. All actual simulation logic lives in pulsive-core.
//!
//! # Deterministic RNG
//!
//! Each core gets a deterministic RNG derived from:
//! - Base seed (from group configuration)
//! - Core ID (unique within the group)
//! - Current tick (simulation time)
//!
//! This ensures reproducible results regardless of execution order.

use crate::config::hash_seed;
use pulsive_core::{Model, Rng, Runtime, UpdateResult};
use serde::{Deserialize, Serialize};

/// Unique identifier for a core within a group
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CoreId(pub usize);

impl std::fmt::Display for CoreId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Core({})", self.0)
    }
}

/// Thin wrapper bundling a pulsive-core Runtime + Model
///
/// This is NOT a fork or modification of pulsive-core. It simply holds:
/// - `runtime`: pulsive-core's Runtime, used exactly as-is
/// - `model`: pulsive-core's Model, used exactly as-is
/// - `id`: identifier for this instance within a CoreGroup
/// - `rng_seed`: seed for deterministic RNG per core
///
/// The `tick()` method delegates directly to `runtime.tick(&mut model)`.
pub struct Core {
    /// Identifier within a CoreGroup
    pub id: CoreId,
    /// pulsive-core Runtime (used as-is, no modifications)
    pub runtime: Runtime,
    /// pulsive-core Model (used as-is, no modifications)
    pub model: Model,
    /// Seed for deterministic per-core RNG
    rng_seed: u64,
}

impl Core {
    /// Create a new core with the given runtime and RNG seed
    pub fn new(id: CoreId, runtime: Runtime, seed: u64) -> Self {
        Self {
            id,
            runtime,
            model: Model::new(),
            rng_seed: seed,
        }
    }

    /// Create a core with default runtime
    pub fn with_seed(id: CoreId, seed: u64) -> Self {
        Self::new(id, Runtime::new(), seed)
    }

    /// Get the runtime (for registering handlers)
    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    /// Get mutable runtime (for registering handlers)
    pub fn runtime_mut(&mut self) -> &mut Runtime {
        &mut self.runtime
    }

    /// Get the local model
    pub fn model(&self) -> &Model {
        &self.model
    }

    /// Load a model snapshot into this core's local model
    pub fn load_model(&mut self, model: Model) {
        self.model = model;
        // Reset RNG with seed + current tick for determinism
        let tick = self.model.current_tick();
        self.model.rng = Rng::new(hash_seed(self.rng_seed, self.id.0 as u64, tick));
    }

    /// Execute one tick - delegates directly to `runtime.tick(&mut model)`
    pub fn tick(&mut self) -> UpdateResult {
        self.runtime.tick(&mut self.model)
    }

    /// Get the current tick of the local model
    pub fn current_tick(&self) -> u64 {
        self.model.current_tick()
    }

    /// Re-seed the RNG for a new tick
    pub fn reseed_rng(&mut self, tick: u64) {
        self.model.rng = Rng::new(hash_seed(self.rng_seed, self.id.0 as u64, tick));
    }
}

// Note: hash_seed is imported from crate::config

impl std::fmt::Debug for Core {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Core")
            .field("id", &self.id)
            .field("tick", &self.model.current_tick())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_core_creation() {
        let core = Core::with_seed(CoreId(0), 12345);
        assert_eq!(core.id, CoreId(0));
        assert_eq!(core.current_tick(), 0);
    }

    #[test]
    fn test_core_tick() {
        let mut core = Core::with_seed(CoreId(0), 12345);
        core.tick();
        assert_eq!(core.current_tick(), 1);
    }

    #[test]
    fn test_hash_seed_determinism() {
        // Same inputs should produce same output
        assert_eq!(hash_seed(100, 0, 5), hash_seed(100, 0, 5));

        // Different inputs should produce different outputs
        assert_ne!(hash_seed(100, 0, 5), hash_seed(100, 1, 5));
        assert_ne!(hash_seed(100, 0, 5), hash_seed(100, 0, 6));
        assert_ne!(hash_seed(100, 0, 5), hash_seed(101, 0, 5));
    }
}
