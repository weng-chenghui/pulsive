//! Hub - Central coordinator for parallel execution
//!
//! Hub owns the global model and coordinates CoreGroups.
//! It never interacts with individual Cores directly.
//!
//! ## Thread Configuration
//!
//! The Hub supports configurable core counts for parallel execution.
//! The `core_count` setting controls how many worker cores will be used
//! when parallel execution is implemented. Currently stored for future use.

use crate::config::{max_cores, HubConfig};
use crate::error::{Error, Result};
use crate::group::{CoreGroup, GroupId};
use crate::snapshot::ModelSnapshot;
use crate::tick_sync::TickSyncGroup;
use pulsive_core::{Model, UpdateResult};

/// Result of a hub tick
#[derive(Debug, Clone)]
pub struct TickResult {
    /// The tick that was executed
    pub tick: u64,
    /// Combined update results from all groups
    pub updates: Vec<UpdateResult>,
}

/// Central coordinator that owns the global model and manages CoreGroups
///
/// Hub responsibilities:
/// - Own and manage the global model
/// - Create snapshots for groups
/// - Merge changes from groups back to global model
/// - Configure thread/core count for parallel execution
/// - (Future) Handle journal integration
/// - (Future) Handle rollback requests
///
/// ## Thread Configuration
///
/// The Hub supports configurable core counts. This setting is stored for
/// when parallel execution is implemented.
///
/// ```
/// use pulsive_hub::Hub;
///
/// let mut hub = Hub::new();
///
/// // Default is single-core (core_count == 1)
/// assert_eq!(hub.core_count(), 1);
///
/// // Configure for 4 cores (for future parallel execution)
/// hub.set_core_count(4);
/// assert_eq!(hub.core_count(), 4.min(pulsive_hub::max_cores()));
///
/// // Can change between ticks
/// hub.set_core_count(1);
/// assert_eq!(hub.core_count(), 1);
/// ```
pub struct Hub {
    /// The global model (source of truth)
    model: Model,
    /// Core groups (Hub owns these, never individual cores)
    groups: Vec<Box<dyn CoreGroup>>,
    /// Version counter for MVCC
    version: u64,
    /// Runtime configuration including thread count
    config: HubConfig,
}

impl Hub {
    /// Create a new hub with an empty model
    ///
    /// The hub starts in single-core mode (zero parallel overhead).
    pub fn new() -> Self {
        Self {
            model: Model::new(),
            groups: Vec::new(),
            version: 0,
            config: HubConfig::default(),
        }
    }

    /// Create a hub with an initial model
    ///
    /// The hub starts in single-core mode (zero parallel overhead).
    pub fn with_model(model: Model) -> Self {
        Self {
            model,
            groups: Vec::new(),
            version: 0,
            config: HubConfig::default(),
        }
    }

    /// Create a hub with a specific configuration
    ///
    /// # Arguments
    ///
    /// * `model` - Initial model
    /// * `config` - Hub configuration including core count
    pub fn with_config(model: Model, config: HubConfig) -> Self {
        Self {
            model,
            groups: Vec::new(),
            version: 0,
            config,
        }
    }

    /// Create a hub with a default single-core group
    ///
    /// The hub starts in single-core mode (zero parallel overhead).
    pub fn with_default_group(model: Model, seed: u64) -> Self {
        let mut hub = Self::with_model(model);
        hub.add_group(TickSyncGroup::single(GroupId(0), seed));
        hub
    }

    /// Add a core group to the hub
    pub fn add_group(&mut self, group: impl CoreGroup + 'static) {
        self.groups.push(Box::new(group));
    }

    /// Get the number of groups
    pub fn group_count(&self) -> usize {
        self.groups.len()
    }

    /// Get a reference to the global model
    pub fn model(&self) -> &Model {
        &self.model
    }

    /// Get a mutable reference to the global model
    pub fn model_mut(&mut self) -> &mut Model {
        &mut self.model
    }

    /// Get the current version
    pub fn version(&self) -> u64 {
        self.version
    }

    // ========================================================================
    // Thread Configuration API
    // ========================================================================

    /// Set number of worker cores
    ///
    /// The value is clamped to `[1, max_cores()]`.
    ///
    /// This setting is stored for when parallel execution is implemented.
    /// Currently, execution behavior is the same regardless of core count.
    ///
    /// # Example
    ///
    /// ```
    /// use pulsive_hub::Hub;
    ///
    /// let mut hub = Hub::new();
    ///
    /// // Configure for 4 cores
    /// hub.set_core_count(4);
    /// assert_eq!(hub.core_count(), 4.min(pulsive_hub::max_cores()));
    ///
    /// // Can change between ticks
    /// hub.set_core_count(1);
    /// assert_eq!(hub.core_count(), 1);
    /// ```
    pub fn set_core_count(&mut self, n: usize) {
        self.config.set_core_count(n);
    }

    /// Get current core count
    ///
    /// Returns the number of worker cores configured for parallel execution.
    ///
    /// # Example
    ///
    /// ```
    /// use pulsive_hub::Hub;
    ///
    /// let hub = Hub::new();
    /// assert_eq!(hub.core_count(), 1); // Default is 1
    /// ```
    pub fn core_count(&self) -> usize {
        self.config.core_count()
    }

    /// Get maximum available cores on this system
    ///
    /// This is a convenience method that delegates to [`max_cores()`].
    ///
    /// # Example
    ///
    /// ```
    /// use pulsive_hub::Hub;
    ///
    /// let hub = Hub::new();
    /// let max = hub.max_cores();
    /// assert!(max >= 1);
    /// ```
    pub fn max_cores(&self) -> usize {
        max_cores()
    }

    /// Get a reference to the hub configuration
    pub fn config(&self) -> &HubConfig {
        &self.config
    }

    /// Get a mutable reference to the hub configuration
    pub fn config_mut(&mut self) -> &mut HubConfig {
        &mut self.config
    }

    /// Get the global seed
    ///
    /// Returns the master seed used for deriving per-core RNG seeds.
    ///
    /// # Example
    ///
    /// ```
    /// use pulsive_hub::{Hub, HubConfig};
    /// use pulsive_core::Model;
    ///
    /// let config = HubConfig::with_seed(42);
    /// let hub = Hub::with_config(Model::new(), config);
    /// assert_eq!(hub.global_seed(), 42);
    /// ```
    pub fn global_seed(&self) -> u64 {
        self.config.global_seed()
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
    /// use pulsive_hub::Hub;
    ///
    /// let mut hub = Hub::new();
    /// hub.set_global_seed(42);
    /// assert_eq!(hub.global_seed(), 42);
    /// ```
    pub fn set_global_seed(&mut self, seed: u64) {
        self.config.set_global_seed(seed);
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
    /// * `tick` - The simulation tick
    ///
    /// # Example
    ///
    /// ```
    /// use pulsive_hub::Hub;
    ///
    /// let mut hub = Hub::new();
    /// hub.set_global_seed(42);
    ///
    /// // Same inputs produce same RNG
    /// let mut rng1 = hub.create_core_rng(0, 5);
    /// let mut rng2 = hub.create_core_rng(0, 5);
    /// assert_eq!(rng1.next_u64(), rng2.next_u64());
    ///
    /// // Different cores get different RNG streams
    /// let mut rng_core0 = hub.create_core_rng(0, 5);
    /// let mut rng_core1 = hub.create_core_rng(1, 5);
    /// assert_ne!(rng_core0.next_u64(), rng_core1.next_u64());
    /// ```
    pub fn create_core_rng(&self, core_id: usize, tick: u64) -> pulsive_core::Rng {
        self.config.create_core_rng(core_id, tick)
    }

    // ========================================================================
    // Snapshot and Tick
    // ========================================================================

    /// Create a snapshot of the current model state
    pub fn snapshot(&self) -> ModelSnapshot {
        ModelSnapshot::new(&self.model, self.version)
    }

    /// Execute one tick across all groups
    ///
    /// Flow:
    /// 1. Load current model into each group's cores
    /// 2. Execute tick on all groups
    /// 3. Merge results back to global model
    /// 4. Advance version
    ///
    /// # Execution Mode
    ///
    /// The execution strategy is selected based on `core_count`:
    /// - `core_count == 1`: Sequential execution with zero parallel overhead
    /// - `core_count > 1`: Parallel execution (when driver is implemented)
    ///
    /// See [Issue #55](https://github.com/weng-chenghui/pulsive/issues/55) for
    /// the ExecutionDriver abstraction that will enable swappable drivers
    /// (LocalDriver, RayonDriver, etc.).
    ///
    /// # Example
    ///
    /// ```
    /// use pulsive_hub::{Hub, TickSyncGroup, GroupId};
    /// use pulsive_core::Model;
    ///
    /// let mut hub = Hub::with_default_group(Model::new(), 12345);
    ///
    /// let result = hub.tick().unwrap();
    /// assert_eq!(result.tick, 1);
    ///
    /// let result = hub.tick().unwrap();
    /// assert_eq!(result.tick, 2);
    /// ```
    pub fn tick(&mut self) -> Result<TickResult> {
        if self.groups.is_empty() {
            return Err(Error::NoGroups);
        }

        // Dispatch based on core_count configuration
        // See Issue #55 for ExecutionDriver trait abstraction
        if self.config.core_count() == 1 {
            self.tick_sequential()
        } else {
            self.tick_parallel()
        }
    }

    /// Sequential tick execution (single-core mode)
    ///
    /// This is the zero-overhead path for single-core mode.
    /// No thread pool, no parallel infrastructure.
    fn tick_sequential(&mut self) -> Result<TickResult> {
        let mut all_updates = Vec::new();

        for group in &mut self.groups {
            // Load current model into group's cores
            group.load_model(&self.model);

            // Execute tick (group handles its cores)
            let updates = group.execute_tick();
            all_updates.extend(updates);

            // Extract the modified model from the group
            // TODO: Implement proper MVCC merge when multiple cores produce WriteSets
            let models = group.extract_models();
            if let Some(modified_model) = models.first() {
                self.model = (*modified_model).clone();
            }

            // Advance group tick
            group.advance_tick();
        }

        // Advance version
        self.version += 1;

        Ok(TickResult {
            tick: self.model.current_tick(),
            updates: all_updates,
        })
    }

    /// Parallel tick execution (multi-core mode)
    ///
    /// This path is used when `core_count > 1`.
    ///
    /// # Current Implementation
    ///
    /// Currently delegates to sequential execution. When the ExecutionDriver
    /// abstraction is implemented (Issue #55), this will use RayonDriver
    /// (Issue #58) or other parallel drivers.
    ///
    /// # Future Implementation
    ///
    /// Will use the configured ExecutionDriver to parallelize core execution
    /// within groups, respecting the `core_count` setting for thread pool size.
    fn tick_parallel(&mut self) -> Result<TickResult> {
        // TODO(#55): Use ExecutionDriver for parallel execution
        // TODO(#58): Implement RayonDriver for rayon-based parallelism
        //
        // For now, delegate to sequential execution.
        // The dispatch structure is in place for when drivers are implemented.
        self.tick_sequential()
    }

    /// Get the current tick from the global model
    pub fn current_tick(&self) -> u64 {
        self.model.current_tick()
    }
}

impl Default for Hub {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Hub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hub")
            .field("tick", &self.model.current_tick())
            .field("version", &self.version)
            .field("groups", &self.groups.len())
            .field("core_count", &self.config.core_count())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsive_core::{DefId, Effect, Expr, TickHandler};

    #[test]
    fn test_hub_creation() {
        let hub = Hub::new();
        assert_eq!(hub.current_tick(), 0);
        assert_eq!(hub.group_count(), 0);
    }

    #[test]
    fn test_hub_with_default_group() {
        let hub = Hub::with_default_group(Model::new(), 12345);
        assert_eq!(hub.group_count(), 1);
    }

    #[test]
    fn test_hub_tick_no_groups() {
        let mut hub = Hub::new();
        let result = hub.tick();
        assert!(matches!(result, Err(Error::NoGroups)));
    }

    #[test]
    fn test_hub_tick() {
        let mut hub = Hub::with_default_group(Model::new(), 12345);

        // Run a tick
        let result = hub.tick();
        assert!(result.is_ok());

        let tick_result = result.unwrap();
        assert_eq!(tick_result.tick, 1);
    }

    #[test]
    fn test_hub_tick_with_handler() {
        let model = Model::new();
        let mut group = TickSyncGroup::single(GroupId(0), 12345);

        // Register a tick handler that increments a global counter
        group.on_tick(TickHandler {
            id: DefId::new("counter"),
            condition: None,
            target_kind: None,
            effects: vec![Effect::ModifyGlobal {
                property: "count".to_string(),
                op: pulsive_core::effect::ModifyOp::Add,
                value: Expr::lit(1.0),
            }],
            priority: 0,
        });

        let mut hub = Hub::with_model(model);
        hub.model_mut().set_global("count", 0.0f64);
        hub.add_group(group);

        // Run 3 ticks
        hub.tick().unwrap();
        hub.tick().unwrap();
        hub.tick().unwrap();

        // Check counter
        let count = hub.model().get_global("count").and_then(|v| v.as_float());
        assert_eq!(count, Some(3.0));
    }

    // ========================================================================
    // Thread Configuration API Tests
    // ========================================================================

    #[test]
    fn test_default_is_single_core() {
        let hub = Hub::new();
        assert_eq!(hub.core_count(), 1);
    }

    #[test]
    fn test_with_config() {
        let config = HubConfig::with_core_count(4);
        let hub = Hub::with_config(Model::new(), config);
        assert_eq!(hub.core_count(), 4.min(max_cores()));
    }

    #[test]
    fn test_set_core_count() {
        let mut hub = Hub::new();
        assert_eq!(hub.core_count(), 1);

        hub.set_core_count(4);
        let expected = 4.min(max_cores());
        assert_eq!(hub.core_count(), expected);

        // Set back to single core
        hub.set_core_count(1);
        assert_eq!(hub.core_count(), 1);
    }

    #[test]
    fn test_max_cores() {
        let hub = Hub::new();
        let max = hub.max_cores();
        assert!(max >= 1, "max_cores should be at least 1");
    }

    #[test]
    fn test_core_count_clamped_minimum() {
        let mut hub = Hub::new();
        hub.set_core_count(0);
        // 0 should be clamped to 1
        assert_eq!(hub.core_count(), 1);
    }

    #[test]
    fn test_core_count_clamped_maximum() {
        let mut hub = Hub::new();
        hub.set_core_count(10000);
        // Should be clamped to max_cores
        assert_eq!(hub.core_count(), max_cores());
    }

    #[test]
    fn test_can_change_core_count_between_ticks() {
        let mut hub = Hub::with_default_group(Model::new(), 12345);

        // Start in single-core mode
        assert_eq!(hub.core_count(), 1);
        hub.tick().unwrap();

        // Switch to parallel mode
        hub.set_core_count(4);
        assert_eq!(hub.core_count(), 4.min(max_cores()));
        hub.tick().unwrap();

        // Switch back to single-core
        hub.set_core_count(1);
        assert_eq!(hub.core_count(), 1);
        hub.tick().unwrap();

        // Verify ticks advanced correctly
        assert_eq!(hub.current_tick(), 3);
    }

    #[test]
    fn test_deterministic_regardless_of_core_count() {
        // Helper to create a tick handler that increments a counter
        fn counter_handler() -> TickHandler {
            TickHandler {
                id: DefId::new("counter"),
                condition: None,
                target_kind: None,
                effects: vec![Effect::ModifyGlobal {
                    property: "count".to_string(),
                    op: pulsive_core::effect::ModifyOp::Add,
                    value: Expr::lit(1.0),
                }],
                priority: 0,
            }
        }

        // Run simulation in single-core mode (core_count = 1)
        let mut group1 = TickSyncGroup::single(GroupId(0), 12345);
        group1.on_tick(counter_handler());

        let mut hub1 = Hub::with_model(Model::new());
        hub1.model_mut().set_global("count", 0.0f64);
        hub1.add_group(group1);
        assert_eq!(hub1.core_count(), 1); // Verify single-core mode

        for _ in 0..5 {
            hub1.tick().unwrap();
        }
        let count1 = hub1.model().get_global("count").and_then(|v| v.as_float());

        // Run same simulation with different core count configuration.
        // Note: The core_count setting is stored for future parallel execution.
        // Currently both paths execute identically since parallel dispatch
        // is not yet implemented - this test verifies determinism is preserved
        // when the setting changes.
        let mut group2 = TickSyncGroup::single(GroupId(0), 12345);
        group2.on_tick(counter_handler());

        let mut hub2 = Hub::with_model(Model::new());
        hub2.model_mut().set_global("count", 0.0f64);
        hub2.add_group(group2);
        hub2.set_core_count(4); // Set for future parallel mode
        assert!(hub2.core_count() > 1 || max_cores() == 1);

        for _ in 0..5 {
            hub2.tick().unwrap();
        }
        let count2 = hub2.model().get_global("count").and_then(|v| v.as_float());

        // Results should be deterministic regardless of core count setting
        assert_eq!(count1, count2);
        assert_eq!(count1, Some(5.0));
    }

    #[test]
    fn test_config_accessors() {
        let mut hub = Hub::new();

        // Test config accessor
        assert_eq!(hub.config().core_count(), 1);

        // Test mutable config accessor
        hub.config_mut().set_core_count(2);
        assert_eq!(hub.core_count(), 2.min(max_cores()));
    }

    // ========================================================================
    // Global Seed and RNG Tests
    // ========================================================================

    #[test]
    fn test_global_seed_accessors() {
        let mut hub = Hub::new();

        // Default seed
        assert_eq!(hub.global_seed(), crate::config::DEFAULT_GLOBAL_SEED);

        // Set custom seed
        hub.set_global_seed(42);
        assert_eq!(hub.global_seed(), 42);
    }

    #[test]
    fn test_create_core_rng_deterministic() {
        let mut hub = Hub::new();
        hub.set_global_seed(42);

        // Same inputs produce same RNG sequence
        let mut rng1 = hub.create_core_rng(0, 5);
        let mut rng2 = hub.create_core_rng(0, 5);

        assert_eq!(rng1.next_u64(), rng2.next_u64());
        assert_eq!(rng1.next_u64(), rng2.next_u64());
        assert_eq!(rng1.next_u64(), rng2.next_u64());
    }

    #[test]
    fn test_create_core_rng_independent_cores() {
        let mut hub = Hub::new();
        hub.set_global_seed(42);

        // Different cores get different RNG streams
        let mut rng_core0 = hub.create_core_rng(0, 5);
        let mut rng_core1 = hub.create_core_rng(1, 5);

        assert_ne!(rng_core0.next_u64(), rng_core1.next_u64());
    }

    #[test]
    fn test_create_core_rng_independent_ticks() {
        let mut hub = Hub::new();
        hub.set_global_seed(42);

        // Different ticks get different RNG streams
        let mut rng_tick5 = hub.create_core_rng(0, 5);
        let mut rng_tick6 = hub.create_core_rng(0, 6);

        assert_ne!(rng_tick5.next_u64(), rng_tick6.next_u64());
    }

    // ========================================================================
    // Per-Core Deterministic RNG - Acceptance Criteria Tests
    // ========================================================================

    /// Test: Same seed + same inputs = same outputs
    ///
    /// Running the same simulation with the same seed should produce
    /// identical results every time.
    #[test]
    fn test_same_seed_produces_same_outputs() {
        fn run_simulation(seed: u64) -> Vec<f64> {
            let mut results = Vec::new();
            let config = HubConfig::with_seed(seed);
            let hub = Hub::with_config(Model::new(), config);

            // Generate 10 random values across different cores and ticks
            for tick in 0..5 {
                for core in 0..2 {
                    let mut rng = hub.create_core_rng(core, tick);
                    results.push(rng.next_f64());
                }
            }
            results
        }

        let run1 = run_simulation(12345);
        let run2 = run_simulation(12345);

        assert_eq!(run1, run2, "Same seed should produce identical outputs");
    }

    /// Test: Different seeds produce different outputs
    #[test]
    fn test_different_seeds_produce_different_outputs() {
        fn run_simulation(seed: u64) -> Vec<f64> {
            let config = HubConfig::with_seed(seed);
            let hub = Hub::with_config(Model::new(), config);

            (0..10)
                .map(|i| {
                    let mut rng = hub.create_core_rng(0, i);
                    rng.next_f64()
                })
                .collect()
        }

        let run1 = run_simulation(12345);
        let run2 = run_simulation(54321);

        assert_ne!(
            run1, run2,
            "Different seeds should produce different outputs"
        );
    }

    /// Test: RNG streams are independent between cores
    ///
    /// Each core should have its own independent RNG stream that doesn't
    /// interfere with other cores.
    #[test]
    fn test_rng_streams_independent_between_cores() {
        let hub = Hub::with_config(Model::new(), HubConfig::with_seed(42));

        // Generate sequences from multiple cores at the same tick
        let sequences: Vec<Vec<u64>> = (0..4)
            .map(|core| {
                let mut rng = hub.create_core_rng(core, 10);
                (0..5).map(|_| rng.next_u64()).collect()
            })
            .collect();

        // All cores should have different sequences
        for i in 0..sequences.len() {
            for j in (i + 1)..sequences.len() {
                assert_ne!(
                    sequences[i], sequences[j],
                    "Core {} and {} should have different RNG sequences",
                    i, j
                );
            }
        }
    }

    /// Test: Replay produces identical results
    ///
    /// Re-running the same core at the same tick should produce the same
    /// RNG sequence, enabling deterministic replay.
    #[test]
    fn test_replay_produces_identical_results() {
        let hub = Hub::with_config(Model::new(), HubConfig::with_seed(99999));

        // First run
        let first_run: Vec<u64> = {
            let mut rng = hub.create_core_rng(3, 42);
            (0..100).map(|_| rng.next_u64()).collect()
        };

        // Replay (same core, same tick)
        let replay: Vec<u64> = {
            let mut rng = hub.create_core_rng(3, 42);
            (0..100).map(|_| rng.next_u64()).collect()
        };

        assert_eq!(first_run, replay, "Replay should produce identical results");
    }

    /// Test: Works with any number of cores
    ///
    /// The determinism should hold regardless of how many cores are configured.
    #[test]
    fn test_works_with_any_core_count() {
        let seed = 77777;

        // Test with various core counts
        for core_count in [1, 2, 4, 8, 16, 100] {
            let config = HubConfig::new(core_count, seed);
            let hub = Hub::with_config(Model::new(), config);

            // Actual core count after clamping
            let actual_core_count = hub.core_count();

            // Each core should get unique, deterministic values
            let values: Vec<u64> = (0..actual_core_count)
                .map(|core| {
                    let mut rng = hub.create_core_rng(core, 0);
                    rng.next_u64()
                })
                .collect();

            // All values should be unique
            let unique: std::collections::HashSet<_> = values.iter().collect();
            assert_eq!(
                unique.len(),
                actual_core_count,
                "All cores should get unique RNG values (configured={}, actual={})",
                core_count,
                actual_core_count
            );
        }
    }

    /// Test: RNG distribution quality check
    ///
    /// Verify that the generated random numbers have reasonable distribution.
    #[test]
    fn test_rng_distribution_quality() {
        let hub = Hub::with_config(Model::new(), HubConfig::with_seed(12345));

        let mut values = Vec::new();
        for tick in 0..100 {
            for core in 0..10 {
                let mut rng = hub.create_core_rng(core, tick);
                values.push(rng.next_f64());
            }
        }

        // Check that values are in [0, 1)
        assert!(values.iter().all(|&v| (0.0..1.0).contains(&v)));

        // Check basic distribution (should be roughly uniform)
        let mean: f64 = values.iter().sum::<f64>() / values.len() as f64;
        assert!(
            (0.4..0.6).contains(&mean),
            "Mean should be close to 0.5, got {}",
            mean
        );

        // Check that we don't have too many duplicates
        let unique: std::collections::HashSet<u64> = values.iter().map(|v| v.to_bits()).collect();
        assert!(
            unique.len() > values.len() * 9 / 10,
            "Should have mostly unique values"
        );
    }

    /// Test: Tick handler with random effects produces deterministic results
    #[test]
    fn test_tick_handler_with_random_deterministic() {
        fn run_random_simulation(seed: u64) -> f64 {
            let mut group = TickSyncGroup::single(GroupId(0), seed);

            // Register a tick handler that uses random values
            group.on_tick(TickHandler {
                id: DefId::new("random_modifier"),
                condition: None,
                target_kind: None,
                effects: vec![Effect::ModifyGlobal {
                    property: "value".to_string(),
                    op: pulsive_core::effect::ModifyOp::Add,
                    // This adds a random value each tick
                    value: Expr::Random,
                }],
                priority: 0,
            });

            let mut hub = Hub::with_model(Model::new());
            hub.model_mut().set_global("value", 0.0f64);
            hub.set_global_seed(seed);
            hub.add_group(group);

            // Run 10 ticks
            for _ in 0..10 {
                hub.tick().unwrap();
            }

            hub.model()
                .get_global("value")
                .and_then(|v| v.as_float())
                .unwrap()
        }

        // Same seed should produce same result
        let result1 = run_random_simulation(12345);
        let result2 = run_random_simulation(12345);
        assert_eq!(
            result1, result2,
            "Same seed should produce same random results"
        );

        // Different seed should produce different result
        let result3 = run_random_simulation(54321);
        assert_ne!(
            result1, result3,
            "Different seed should produce different results"
        );
    }
}
