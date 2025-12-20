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
        // Run simulation in single-core mode
        let mut hub1 = Hub::with_default_group(Model::new(), 12345);
        hub1.model_mut().set_global("count", 0.0f64);

        let mut group1 = TickSyncGroup::single(GroupId(0), 12345);
        group1.on_tick(TickHandler {
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
        let mut hub1 = Hub::with_model(Model::new());
        hub1.model_mut().set_global("count", 0.0f64);
        hub1.add_group(group1);

        for _ in 0..5 {
            hub1.tick().unwrap();
        }
        let count1 = hub1.model().get_global("count").and_then(|v| v.as_float());

        // Run same simulation with different core count configuration
        // (actual parallel execution isn't different here since TickSyncGroup
        // handles its own core count, but the dispatch path is exercised)
        let mut group2 = TickSyncGroup::single(GroupId(0), 12345);
        group2.on_tick(TickHandler {
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
        let mut hub2 = Hub::with_model(Model::new());
        hub2.model_mut().set_global("count", 0.0f64);
        hub2.add_group(group2);
        hub2.set_core_count(4); // Enable parallel mode

        for _ in 0..5 {
            hub2.tick().unwrap();
        }
        let count2 = hub2.model().get_global("count").and_then(|v| v.as_float());

        // Results should be deterministic regardless of core count
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
}
