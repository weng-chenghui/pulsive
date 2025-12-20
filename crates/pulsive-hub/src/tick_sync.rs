//! TickSyncGroup - All cores synchronized at the same tick
//!
//! This is the primary execution model where:
//! - All cores process the same tick
//! - Barrier synchronization ensures all complete before advancing
//! - Single-core mode has zero parallel overhead
//!
//! # Deterministic RNG
//!
//! Each core in the group gets a deterministic RNG derived from:
//! - `base_seed`: The group's seed
//! - `core_id`: The core's index within the group
//! - `tick`: The current simulation tick
//!
//! This ensures reproducible results when replaying simulations.

use crate::core::{Core, CoreId};
use crate::group::{CoreGroup, GroupId};
use pulsive_core::{Model, Runtime, UpdateResult};

/// A group where all cores stay synchronized at the same tick
///
/// Execution flow:
/// 1. All cores load the same snapshot
/// 2. All cores execute tick (parallel if multiple cores)
/// 3. Barrier: wait for all cores to complete
/// 4. Advance tick
pub struct TickSyncGroup {
    /// Unique identifier for this group
    id: GroupId,
    /// Current tick (all cores are at this tick)
    tick: u64,
    /// Cores owned by this group
    cores: Vec<Core>,
    /// Base seed for RNG
    base_seed: u64,
}

impl TickSyncGroup {
    /// Create a new group with the given cores
    pub fn new(id: GroupId, cores: Vec<Core>, base_seed: u64) -> Self {
        Self {
            id,
            tick: 0,
            cores,
            base_seed,
        }
    }

    /// Create a group with N cores using default runtime
    ///
    /// Each core stores the `base_seed` and derives its per-tick RNG using:
    /// `hash(base_seed, core_id, tick)`
    ///
    /// This matches the formula used by `Hub::create_core_rng()`.
    pub fn with_core_count(id: GroupId, count: usize, base_seed: u64) -> Self {
        let cores = (0..count)
            .map(|i| {
                let core_id = CoreId(i);
                // Store the base_seed directly - Core will hash it with (core_id, tick)
                // when load_model() is called. This ensures the formula matches
                // Hub::create_core_rng(core_id, tick) = hash(base_seed, core_id, tick)
                Core::with_seed(core_id, base_seed)
            })
            .collect();

        Self::new(id, cores, base_seed)
    }

    /// Create a single-core group (simplest case)
    pub fn single(id: GroupId, seed: u64) -> Self {
        Self::with_core_count(id, 1, seed)
    }

    /// Add a core to this group
    pub fn add_core(&mut self, core: Core) {
        self.cores.push(core);
    }

    /// Get a reference to the cores (for registering handlers)
    pub fn cores(&self) -> &[Core] {
        &self.cores
    }

    /// Get mutable reference to the cores (for registering handlers)
    pub fn cores_mut(&mut self) -> &mut [Core] {
        &mut self.cores
    }

    /// Get the base seed used for RNG derivation
    ///
    /// Each core's RNG is derived using: `hash(base_seed, core_id, tick)`
    pub fn base_seed(&self) -> u64 {
        self.base_seed
    }

    /// Register an event handler on all cores
    pub fn on_event(&mut self, handler: pulsive_core::EventHandler) {
        for core in &mut self.cores {
            core.runtime_mut().on_event(handler.clone());
        }
    }

    /// Register a tick handler on all cores
    pub fn on_tick(&mut self, handler: pulsive_core::TickHandler) {
        for core in &mut self.cores {
            core.runtime_mut().on_tick(handler.clone());
        }
    }

    /// Create a TickSyncGroup from an existing runtime
    ///
    /// This is useful when you want to reuse a configured runtime.
    /// The `seed` is stored as the base seed and used with the formula:
    /// `hash(seed, core_id, tick)` to derive per-tick RNG.
    pub fn from_runtime(id: GroupId, runtime: Runtime, seed: u64) -> Self {
        // Store base_seed directly - Core will hash it with (core_id, tick)
        let core = Core::new(CoreId(0), runtime, seed);
        Self::new(id, vec![core], seed)
    }
}

impl CoreGroup for TickSyncGroup {
    fn id(&self) -> GroupId {
        self.id
    }

    fn tick(&self) -> u64 {
        self.tick
    }

    fn core_count(&self) -> usize {
        self.cores.len()
    }

    fn load_model(&mut self, model: &Model) {
        for core in &mut self.cores {
            core.load_model(model.clone());
        }
    }

    fn execute_tick(&mut self) -> Vec<UpdateResult> {
        if self.cores.len() == 1 {
            // Single core - direct execution, no overhead
            let result = self.cores[0].tick();
            vec![result]
        } else {
            // Multiple cores - for now, execute serially
            // TODO: Add parallel execution with rayon when needed
            self.cores.iter_mut().map(|core| core.tick()).collect()
        }
    }

    fn extract_models(&self) -> Vec<&Model> {
        self.cores.iter().map(|core| core.model()).collect()
    }

    fn advance_tick(&mut self) {
        self.tick += 1;
        // Note: RNG reseeding happens in load_model() at the start of the next tick.
        // This ensures the RNG is seeded based on the model's actual tick, not the
        // group's tick counter. The formula is: hash(base_seed, core_id, model_tick)
    }
}

impl std::fmt::Debug for TickSyncGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TickSyncGroup")
            .field("id", &self.id)
            .field("tick", &self.tick)
            .field("core_count", &self.cores.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_core_group() {
        let group = TickSyncGroup::single(GroupId(0), 12345);
        assert_eq!(group.core_count(), 1);
        assert_eq!(group.tick(), 0);
    }

    #[test]
    fn test_multi_core_group() {
        let group = TickSyncGroup::with_core_count(GroupId(0), 4, 12345);
        assert_eq!(group.core_count(), 4);
    }

    #[test]
    fn test_execute_tick() {
        let mut group = TickSyncGroup::single(GroupId(0), 12345);

        // Load empty model
        let model = Model::new();
        group.load_model(&model);

        // Execute tick
        let results = group.execute_tick();
        assert_eq!(results.len(), 1);

        // Check tick advanced in core's local model
        let models = group.extract_models();
        assert_eq!(models[0].current_tick(), 1);
    }

    #[test]
    fn test_advance_tick() {
        let mut group = TickSyncGroup::single(GroupId(0), 12345);
        assert_eq!(group.tick(), 0);

        group.advance_tick();
        assert_eq!(group.tick(), 1);

        group.advance_tick();
        assert_eq!(group.tick(), 2);
    }
}
