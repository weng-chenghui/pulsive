//! Hub - Central coordinator for parallel execution
//!
//! Hub owns the global model and coordinates CoreGroups.
//! It never interacts with individual Cores directly.

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
/// - (Future) Handle journal integration
/// - (Future) Handle rollback requests
pub struct Hub {
    /// The global model (source of truth)
    model: Model,
    /// Core groups (Hub owns these, never individual cores)
    groups: Vec<Box<dyn CoreGroup>>,
    /// Version counter for MVCC
    version: u64,
}

impl Hub {
    /// Create a new hub with an empty model
    pub fn new() -> Self {
        Self {
            model: Model::new(),
            groups: Vec::new(),
            version: 0,
        }
    }

    /// Create a hub with an initial model
    pub fn with_model(model: Model) -> Self {
        Self {
            model,
            groups: Vec::new(),
            version: 0,
        }
    }

    /// Create a hub with a default single-core group
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

    /// Create a snapshot of the current model state
    pub fn snapshot(&self) -> ModelSnapshot {
        ModelSnapshot::new(&self.model, self.version)
    }

    /// Execute one tick across all groups
    ///
    /// Flow:
    /// 1. Create snapshot of global model
    /// 2. Load snapshot into each group's cores
    /// 3. Execute tick on all groups
    /// 4. Merge results back to global model
    /// 5. Advance version
    pub fn tick(&mut self) -> Result<TickResult> {
        if self.groups.is_empty() {
            return Err(Error::NoGroups);
        }

        let mut all_updates = Vec::new();

        // For each group
        for group in &mut self.groups {
            // 1. Load current model into group's cores
            group.load_model(&self.model);

            // 2. Execute tick (group handles its cores)
            let updates = group.execute_tick();
            all_updates.extend(updates);

            // 3. Extract the modified model from the group
            // For now, with single group, just take the first core's model
            let models = group.extract_models();
            if let Some(modified_model) = models.first() {
                // Update global model with the modified state
                // This is a simple approach - future versions could diff and merge
                self.model = (*modified_model).clone();
            }

            // 4. Advance group tick
            group.advance_tick();
        }

        // 5. Advance version
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
}
