//! ModelSnapshot - Immutable view of the model for parallel reads
//!
//! Currently a simple wrapper around Model::clone().
//! Future optimizations could use:
//! - Arc sharing for unchanged data
//! - Copy-on-write for entities
//! - Structural sharing

use pulsive_core::Model;

/// An immutable snapshot of the model at a point in time
///
/// Used to provide consistent read access to cores during parallel execution.
/// Each core receives a snapshot and can read from it without synchronization.
#[derive(Debug, Clone)]
pub struct ModelSnapshot {
    /// The model state at snapshot time
    model: Model,
    /// The tick when this snapshot was taken
    tick: u64,
    /// Version number (for MVCC)
    version: u64,
}

impl ModelSnapshot {
    /// Create a new snapshot from a model
    pub fn new(model: &Model, version: u64) -> Self {
        Self {
            tick: model.current_tick(),
            model: model.clone(),
            version,
        }
    }

    /// Get the tick this snapshot was taken at
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Get the version number
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Convert to an owned Model for a core to use
    ///
    /// Each core gets its own mutable copy to work with.
    pub fn to_model(&self) -> Model {
        self.model.clone()
    }

    /// Get a reference to the underlying model (for reading)
    pub fn model(&self) -> &Model {
        &self.model
    }
}
