//! CoreGroup trait - The abstraction layer between Hub and Cores
//!
//! Hub only interacts with CoreGroup, never with individual Cores.
//! This allows different execution strategies to be implemented.

use pulsive_core::{Model, UpdateResult};
use serde::{Deserialize, Serialize};

/// Unique identifier for a core group
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GroupId(pub usize);

impl std::fmt::Display for GroupId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Group({})", self.0)
    }
}

/// Trait for groups of cores with different execution strategies
///
/// Hub interacts only with this trait, never with individual Cores.
/// This enables different execution models:
/// - TickSyncGroup: All cores synchronized at same tick
/// - (Future) AsyncGroup: Cores at different ticks
/// - (Future) PipelineGroup: Streaming execution
pub trait CoreGroup: Send {
    /// Get the unique identifier for this group
    fn id(&self) -> GroupId;

    /// Get the current tick for this group
    fn tick(&self) -> u64;

    /// Get the number of cores in this group
    fn core_count(&self) -> usize;

    /// Load a model into all cores in this group
    ///
    /// Each core gets a clone of the model to work with locally.
    fn load_model(&mut self, model: &Model);

    /// Execute one tick on all cores
    ///
    /// The group decides the execution strategy:
    /// - Serial (single core)
    /// - Parallel with barrier sync
    /// - etc.
    ///
    /// Returns combined results from all cores.
    fn execute_tick(&mut self) -> Vec<UpdateResult>;

    /// Extract the modified models from all cores
    ///
    /// After execute_tick, call this to get the mutated models
    /// which can be diffed against the original to produce WriteSets.
    fn extract_models(&self) -> Vec<&Model>;

    /// Advance the tick counter for this group
    fn advance_tick(&mut self);
}
