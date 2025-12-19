//! WriteSet application and commit functionality
//!
//! This module provides functions for applying WriteSets to the Model:
//!
//! - [`apply`]: Apply a single WriteSet directly (no conflict checking)
//! - [`apply_batch`]: Apply multiple WriteSets merged together (no conflict checking)
//! - [`commit`]: Commit a WriteSet with version tracking
//! - [`commit_batch`]: Commit multiple WriteSets with conflict detection/resolution
//!
//! # Design
//!
//! - `WriteSet` and `PendingWrite` types are defined in `pulsive-core`
//! - `apply()` and commit functions live here in `pulsive-hub` because the Hub owns the Model
//! - This separation enables conflict detection and resolution before applying
//!
//! # Example
//!
//! ```rust,ignore
//! use pulsive_hub::{commit_batch, ResolutionStrategy, CoreId};
//!
//! // Collect WriteSets from multiple cores
//! let write_sets = vec![
//!     (CoreId(0), core0_writes),
//!     (CoreId(1), core1_writes),
//! ];
//!
//! // Commit with conflict resolution
//! let result = commit_batch(
//!     write_sets,
//!     &mut model,
//!     &mut version,
//!     &ResolutionStrategy::LastWriteWins,
//! )?;
//!
//! println!("Committed version {}, {} conflicts resolved",
//!     result.version, result.conflicts_resolved);
//! ```

use crate::conflict::{detect_conflicts, resolve_conflicts, ResolutionStrategy};
use crate::{CoreId, Result};
use pulsive_core::{EntityId, Model, PendingWrite, Value, WriteSet, WriteSetResult};

/// Result of a successful commit operation
#[derive(Debug, Clone, Default)]
pub struct CommitResult {
    /// The version number after this commit
    pub version: u64,
    /// Entity IDs that were spawned
    pub spawned: Vec<EntityId>,
    /// Entity IDs that were destroyed
    pub destroyed: Vec<EntityId>,
    /// Number of conflicts that were resolved (0 for single WriteSet)
    pub conflicts_resolved: usize,
}

impl CommitResult {
    /// Create a new empty commit result
    pub fn new(version: u64) -> Self {
        Self {
            version,
            ..Default::default()
        }
    }

    /// Merge write set result into this commit result
    pub fn merge_write_result(&mut self, result: WriteSetResult) {
        self.spawned.extend(result.spawned);
        self.destroyed.extend(result.destroyed);
    }
}

/// Apply a WriteSet to a Model
///
/// This function applies all pending writes from a WriteSet atomically
/// to the given model. The writes are applied in order.
///
/// This is a low-level function that does not perform conflict detection.
/// For parallel execution, use [`commit_batch`] instead.
///
/// # Arguments
///
/// * `write_set` - The WriteSet to apply
/// * `model` - The Model to apply writes to
///
/// # Returns
///
/// A `WriteSetResult` containing:
/// - `spawned`: Entity IDs that were created
/// - `destroyed`: Entity IDs that were removed
pub fn apply(write_set: &WriteSet, model: &mut Model) -> WriteSetResult {
    let mut result = WriteSetResult::new();

    for write in write_set.iter() {
        match write {
            PendingWrite::SetProperty {
                entity_id,
                key,
                value,
            } => {
                if let Some(entity) = model.entities_mut().get_mut(*entity_id) {
                    entity.set(key.clone(), value.clone());
                }
            }

            PendingWrite::ModifyProperty {
                entity_id,
                key,
                op,
                value,
            } => {
                if let Some(entity) = model.entities_mut().get_mut(*entity_id) {
                    let current = entity.get_number(key).unwrap_or(0.0);
                    let new_value = op.apply(current, *value);
                    entity.set(key.clone(), new_value);
                }
            }

            PendingWrite::SetGlobal { key, value } => {
                model.globals_mut().insert(key.clone(), value.clone());
            }

            PendingWrite::ModifyGlobal { key, op, value } => {
                let current = model
                    .globals()
                    .get(key)
                    .and_then(|v| v.as_float())
                    .unwrap_or(0.0);
                let new_value = op.apply(current, *value);
                model
                    .globals_mut()
                    .insert(key.clone(), Value::Float(new_value));
            }

            PendingWrite::AddFlag { entity_id, flag } => {
                if let Some(entity) = model.entities_mut().get_mut(*entity_id) {
                    entity.add_flag(flag.clone());
                }
            }

            PendingWrite::RemoveFlag { entity_id, flag } => {
                if let Some(entity) = model.entities_mut().get_mut(*entity_id) {
                    entity.remove_flag(flag);
                }
            }

            PendingWrite::SpawnEntity { kind, properties } => {
                let entity = model.entities_mut().create(kind.clone());
                let entity_id = entity.id;

                // Set initial properties
                for (key, value) in properties {
                    entity.set(key.clone(), value.clone());
                }

                result.spawned.push(entity_id);
            }

            PendingWrite::DestroyEntity { id } => {
                model.entities_mut().remove(*id);
                result.destroyed.push(*id);
            }
        }
    }

    result
}

/// Apply multiple WriteSets by merging them first
///
/// This is a convenience function for applying results from multiple cores
/// when you know there are no conflicts.
///
/// **Warning**: This does not perform conflict detection. For parallel execution
/// with potential conflicts, use [`commit_batch`] instead.
pub fn apply_batch(write_sets: Vec<WriteSet>, model: &mut Model) -> WriteSetResult {
    let merged = WriteSet::merge(write_sets);
    apply(&merged, model)
}

/// Commit a single WriteSet with version tracking
///
/// This is a simple commit for single-core execution or when you've already
/// resolved conflicts externally.
///
/// # Arguments
///
/// * `write_set` - The WriteSet to commit
/// * `model` - The Model to apply writes to
/// * `version` - Current version, will be incremented on success
///
/// # Returns
///
/// A `CommitResult` with the new version and spawned/destroyed entities.
pub fn commit(write_set: WriteSet, model: &mut Model, version: &mut u64) -> CommitResult {
    // Skip version increment if no writes (no state change)
    if write_set.is_empty() {
        return CommitResult::new(*version);
    }

    let write_result = apply(&write_set, model);

    *version += 1;

    let mut result = CommitResult::new(*version);
    result.merge_write_result(write_result);
    result
}

/// Commit multiple WriteSets from parallel cores with conflict detection/resolution
///
/// This is the main entry point for committing parallel execution results.
/// It detects conflicts between WriteSets, resolves them using the provided
/// strategy, and applies the resolved writes atomically.
///
/// # Arguments
///
/// * `write_sets` - WriteSets from each core, paired with their CoreId
/// * `model` - The Model to apply writes to
/// * `version` - Current version, will be incremented on success
/// * `strategy` - How to resolve conflicts between cores
///
/// # Returns
///
/// * `Ok(CommitResult)` - Commit succeeded with version and statistics
/// * `Err(Error::UnresolvedConflicts)` - Conflicts couldn't be resolved (e.g., with `Abort` strategy)
///
/// # Example
///
/// ```rust,ignore
/// let result = commit_batch(
///     vec![
///         (CoreId(0), core0_writes),
///         (CoreId(1), core1_writes),
///     ],
///     &mut model,
///     &mut version,
///     &ResolutionStrategy::LastWriteWins,
/// )?;
/// ```
pub fn commit_batch(
    write_sets: Vec<(CoreId, WriteSet)>,
    model: &mut Model,
    version: &mut u64,
    strategy: &ResolutionStrategy,
) -> Result<CommitResult> {
    // Fast path: single WriteSet has no conflicts
    if write_sets.len() <= 1 {
        let merged = WriteSet::merge(write_sets.into_iter().map(|(_, ws)| ws).collect());
        return Ok(commit(merged, model, version));
    }

    // Detect and resolve conflicts
    let resolution_result = resolve_conflicts(&write_sets, strategy)?;

    // Skip version increment if no writes (no state change)
    if resolution_result.write_set.is_empty() {
        return Ok(CommitResult::new(*version));
    }

    // Apply the resolved writes
    let write_result = apply(&resolution_result.write_set, model);

    *version += 1;

    let mut result = CommitResult::new(*version);
    result.merge_write_result(write_result);
    result.conflicts_resolved = resolution_result.conflicts_resolved;

    Ok(result)
}

/// Check for conflicts without committing
///
/// Useful for dry-run validation or reporting before commit.
///
/// # Returns
///
/// `true` if there are conflicts, `false` if safe to merge.
pub fn has_conflicts(write_sets: &[(CoreId, WriteSet)]) -> bool {
    detect_conflicts(write_sets).has_conflicts()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    use pulsive_core::{DefId, ModifyOp, ValueMap};

    #[test]
    fn test_apply_set_global() {
        let mut model = Model::new();
        let mut write_set = WriteSet::new();

        write_set.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(100.0),
        });

        apply(&write_set, &mut model);

        assert_eq!(
            model.get_global("gold").and_then(|v| v.as_float()),
            Some(100.0)
        );
    }

    #[test]
    fn test_apply_modify_global() {
        let mut model = Model::new();
        model.set_global("gold", 100.0f64);

        let mut write_set = WriteSet::new();
        write_set.push(PendingWrite::ModifyGlobal {
            key: "gold".to_string(),
            op: ModifyOp::Add,
            value: 50.0,
        });

        apply(&write_set, &mut model);

        assert_eq!(
            model.get_global("gold").and_then(|v| v.as_float()),
            Some(150.0)
        );
    }

    #[test]
    fn test_apply_entity_property() {
        let mut model = Model::new();
        let entity = model.entities_mut().create("nation");
        entity.set("gold", 100.0f64);
        let entity_id = entity.id;

        let mut write_set = WriteSet::new();
        write_set.push(PendingWrite::SetProperty {
            entity_id,
            key: "gold".to_string(),
            value: Value::Float(200.0),
        });

        apply(&write_set, &mut model);

        assert_eq!(
            model
                .entities()
                .get(entity_id)
                .and_then(|e| e.get_number("gold")),
            Some(200.0)
        );
    }

    #[test]
    fn test_apply_modify_entity_property() {
        let mut model = Model::new();
        let entity = model.entities_mut().create("nation");
        entity.set("gold", 100.0f64);
        let entity_id = entity.id;

        let mut write_set = WriteSet::new();
        write_set.push(PendingWrite::ModifyProperty {
            entity_id,
            key: "gold".to_string(),
            op: ModifyOp::Mul,
            value: 2.0,
        });

        apply(&write_set, &mut model);

        assert_eq!(
            model
                .entities()
                .get(entity_id)
                .and_then(|e| e.get_number("gold")),
            Some(200.0)
        );
    }

    #[test]
    fn test_apply_spawn_entity() {
        let mut model = Model::new();

        let mut properties = ValueMap::new();
        properties.insert("name".to_string(), Value::String("France".to_string()));
        properties.insert("gold".to_string(), Value::Float(100.0));

        let mut write_set = WriteSet::new();
        write_set.push(PendingWrite::SpawnEntity {
            kind: DefId::new("nation"),
            properties,
        });

        let result = apply(&write_set, &mut model);

        assert_eq!(result.spawned.len(), 1);
        let entity_id = result.spawned[0];
        let entity = model.entities().get(entity_id).unwrap();
        assert_eq!(entity.get("name").and_then(|v| v.as_str()), Some("France"));
        assert_eq!(entity.get_number("gold"), Some(100.0));
    }

    #[test]
    fn test_apply_destroy_entity() {
        let mut model = Model::new();
        let entity = model.entities_mut().create("nation");
        let entity_id = entity.id;

        assert!(model.entities().get(entity_id).is_some());

        let mut write_set = WriteSet::new();
        write_set.push(PendingWrite::DestroyEntity { id: entity_id });

        let result = apply(&write_set, &mut model);

        assert_eq!(result.destroyed.len(), 1);
        assert!(model.entities().get(entity_id).is_none());
    }

    #[test]
    fn test_apply_flags() {
        let mut model = Model::new();
        let entity = model.entities_mut().create("nation");
        let entity_id = entity.id;

        let mut write_set = WriteSet::new();
        write_set.push(PendingWrite::AddFlag {
            entity_id,
            flag: DefId::new("at_war"),
        });

        apply(&write_set, &mut model);

        let entity = model.entities().get(entity_id).unwrap();
        assert!(entity.has_flag(&DefId::new("at_war")));

        // Remove the flag
        let mut write_set = WriteSet::new();
        write_set.push(PendingWrite::RemoveFlag {
            entity_id,
            flag: DefId::new("at_war"),
        });

        apply(&write_set, &mut model);

        let entity = model.entities().get(entity_id).unwrap();
        assert!(!entity.has_flag(&DefId::new("at_war")));
    }

    #[test]
    fn test_atomic_application_order() {
        // Verify that writes are applied in order
        let mut model = Model::new();
        model.set_global("counter", 0.0f64);

        let mut write_set = WriteSet::new();
        // First add 10
        write_set.push(PendingWrite::ModifyGlobal {
            key: "counter".to_string(),
            op: ModifyOp::Add,
            value: 10.0,
        });
        // Then multiply by 2
        write_set.push(PendingWrite::ModifyGlobal {
            key: "counter".to_string(),
            op: ModifyOp::Mul,
            value: 2.0,
        });

        apply(&write_set, &mut model);

        // Should be (0 + 10) * 2 = 20
        assert_eq!(
            model.get_global("counter").and_then(|v| v.as_float()),
            Some(20.0)
        );
    }

    #[test]
    fn test_apply_batch() {
        let mut model = Model::new();
        model.set_global("total", 0.0f64);

        let mut ws1 = WriteSet::new();
        ws1.push(PendingWrite::ModifyGlobal {
            key: "total".to_string(),
            op: ModifyOp::Add,
            value: 10.0,
        });

        let mut ws2 = WriteSet::new();
        ws2.push(PendingWrite::ModifyGlobal {
            key: "total".to_string(),
            op: ModifyOp::Add,
            value: 20.0,
        });

        apply_batch(vec![ws1, ws2], &mut model);

        assert_eq!(
            model.get_global("total").and_then(|v| v.as_float()),
            Some(30.0)
        );
    }

    // ========================================================================
    // Commit tests
    // ========================================================================

    #[test]
    fn test_commit_single() {
        let mut model = Model::new();
        let mut version = 0u64;

        let mut write_set = WriteSet::new();
        write_set.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(100.0),
        });

        let result = commit(write_set, &mut model, &mut version);

        assert_eq!(result.version, 1);
        assert_eq!(version, 1);
        assert_eq!(
            model.get_global("gold").and_then(|v| v.as_float()),
            Some(100.0)
        );
    }

    #[test]
    fn test_commit_batch_no_conflicts() {
        let mut model = Model::new();
        let mut version = 0u64;

        let mut ws0 = WriteSet::new();
        ws0.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(100.0),
        });

        let mut ws1 = WriteSet::new();
        ws1.push(PendingWrite::SetGlobal {
            key: "silver".to_string(),
            value: Value::Float(200.0),
        });

        let result = commit_batch(
            vec![(CoreId(0), ws0), (CoreId(1), ws1)],
            &mut model,
            &mut version,
            &ResolutionStrategy::Abort,
        )
        .unwrap();

        assert_eq!(result.version, 1);
        assert_eq!(result.conflicts_resolved, 0);
        assert_eq!(
            model.get_global("gold").and_then(|v| v.as_float()),
            Some(100.0)
        );
        assert_eq!(
            model.get_global("silver").and_then(|v| v.as_float()),
            Some(200.0)
        );
    }

    #[test]
    fn test_commit_batch_with_conflicts_abort() {
        let mut model = Model::new();
        let mut version = 0u64;

        let mut ws0 = WriteSet::new();
        ws0.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(100.0),
        });

        let mut ws1 = WriteSet::new();
        ws1.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(200.0),
        });

        let result = commit_batch(
            vec![(CoreId(0), ws0), (CoreId(1), ws1)],
            &mut model,
            &mut version,
            &ResolutionStrategy::Abort,
        );

        assert!(result.is_err());
        assert!(matches!(result, Err(Error::UnresolvedConflicts { .. })));
        // Version should not have changed
        assert_eq!(version, 0);
    }

    #[test]
    fn test_commit_batch_with_conflicts_last_write_wins() {
        let mut model = Model::new();
        let mut version = 0u64;

        let mut ws0 = WriteSet::new();
        ws0.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(100.0),
        });

        let mut ws1 = WriteSet::new();
        ws1.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(200.0),
        });

        let result = commit_batch(
            vec![(CoreId(0), ws0), (CoreId(1), ws1)],
            &mut model,
            &mut version,
            &ResolutionStrategy::LastWriteWins,
        )
        .unwrap();

        assert_eq!(result.version, 1);
        assert_eq!(result.conflicts_resolved, 1);
        // Core 1's value wins (higher CoreId)
        assert_eq!(
            model.get_global("gold").and_then(|v| v.as_float()),
            Some(200.0)
        );
    }

    #[test]
    fn test_commit_batch_with_conflicts_first_write_wins() {
        let mut model = Model::new();
        let mut version = 0u64;

        let mut ws0 = WriteSet::new();
        ws0.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(100.0),
        });

        let mut ws1 = WriteSet::new();
        ws1.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(200.0),
        });

        let result = commit_batch(
            vec![(CoreId(0), ws0), (CoreId(1), ws1)],
            &mut model,
            &mut version,
            &ResolutionStrategy::FirstWriteWins,
        )
        .unwrap();

        assert_eq!(result.version, 1);
        assert_eq!(result.conflicts_resolved, 1);
        // Core 0's value wins (lower CoreId)
        assert_eq!(
            model.get_global("gold").and_then(|v| v.as_float()),
            Some(100.0)
        );
    }

    #[test]
    fn test_commit_batch_single_writeset() {
        // Single WriteSet should take fast path (no conflict detection)
        let mut model = Model::new();
        let mut version = 0u64;

        let mut ws = WriteSet::new();
        ws.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(100.0),
        });

        let result = commit_batch(
            vec![(CoreId(0), ws)],
            &mut model,
            &mut version,
            &ResolutionStrategy::Abort,
        )
        .unwrap();

        assert_eq!(result.version, 1);
        assert_eq!(result.conflicts_resolved, 0);
    }

    #[test]
    fn test_commit_batch_empty_no_version_increment() {
        // Empty commits should NOT increment version (no state change)
        let mut model = Model::new();
        let mut version = 5u64;

        let result =
            commit_batch(vec![], &mut model, &mut version, &ResolutionStrategy::Abort).unwrap();

        // Version should remain unchanged
        assert_eq!(result.version, 5);
        assert_eq!(version, 5);
        assert_eq!(result.spawned.len(), 0);
        assert_eq!(result.destroyed.len(), 0);
    }

    #[test]
    fn test_has_conflicts() {
        let mut ws0 = WriteSet::new();
        ws0.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(100.0),
        });

        let mut ws1 = WriteSet::new();
        ws1.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(200.0),
        });

        let mut ws2 = WriteSet::new();
        ws2.push(PendingWrite::SetGlobal {
            key: "silver".to_string(),
            value: Value::Float(300.0),
        });

        // Conflicting
        assert!(has_conflicts(&[(CoreId(0), ws0.clone()), (CoreId(1), ws1)]));

        // Non-conflicting
        assert!(!has_conflicts(&[(CoreId(0), ws0), (CoreId(2), ws2)]));
    }

    // ========================================================================
    // Integration tests: collect_effect → apply pattern
    // ========================================================================

    use pulsive_core::{effect::EffectResult, Effect, EntityRef, Expr, Runtime};

    /// Test the full deferred write pattern: collect_effect then apply
    #[test]
    fn test_collect_then_apply_global() {
        let mut model = Model::new();
        model.set_global("gold", 100.0f64);

        let mut runtime = Runtime::new();
        let mut effect_result = EffectResult::new();
        let params = ValueMap::new();

        // Create an effect that modifies a global
        let effect = Effect::ModifyGlobal {
            property: "gold".to_string(),
            op: ModifyOp::Add,
            value: Expr::lit(50.0),
        };

        // Phase 1: Collect writes (model not mutated yet)
        let write_set = runtime.collect_effect(
            &mut model,
            &effect,
            &EntityRef::Global,
            &params,
            &mut effect_result,
        );

        // Verify model wasn't mutated during collection
        assert_eq!(
            model.get_global("gold").and_then(|v| v.as_float()),
            Some(100.0),
            "Model should not be mutated during collect phase"
        );

        // Verify we collected the write
        assert_eq!(write_set.len(), 1);

        // Phase 2: Apply writes
        apply(&write_set, &mut model);

        // Now model should be updated
        assert_eq!(
            model.get_global("gold").and_then(|v| v.as_float()),
            Some(150.0)
        );
    }

    /// Test collect → apply for entity property modification
    #[test]
    fn test_collect_then_apply_entity() {
        let mut model = Model::new();
        let entity = model.entities_mut().create("nation");
        entity.set("population", 1000.0f64);
        let entity_id = entity.id;

        let mut runtime = Runtime::new();
        let mut effect_result = EffectResult::new();
        let params = ValueMap::new();

        // Effect to double the population
        let effect = Effect::ModifyProperty {
            property: "population".to_string(),
            op: ModifyOp::Mul,
            value: Expr::lit(2.0),
        };

        let target = EntityRef::Entity(entity_id);

        // Collect
        let write_set =
            runtime.collect_effect(&mut model, &effect, &target, &params, &mut effect_result);

        // Verify not mutated
        assert_eq!(
            model
                .entities()
                .get(entity_id)
                .and_then(|e| e.get_number("population")),
            Some(1000.0)
        );

        // Apply
        apply(&write_set, &mut model);

        assert_eq!(
            model
                .entities()
                .get(entity_id)
                .and_then(|e| e.get_number("population")),
            Some(2000.0)
        );
    }

    /// Test collect → apply for a sequence of effects
    #[test]
    fn test_collect_sequence_then_apply() {
        let mut model = Model::new();
        model.set_global("counter", 0.0f64);

        let mut runtime = Runtime::new();
        let mut effect_result = EffectResult::new();
        let params = ValueMap::new();

        // Sequence: add 10, then multiply by 2
        let effect = Effect::Sequence(vec![
            Effect::ModifyGlobal {
                property: "counter".to_string(),
                op: ModifyOp::Add,
                value: Expr::lit(10.0),
            },
            Effect::ModifyGlobal {
                property: "counter".to_string(),
                op: ModifyOp::Mul,
                value: Expr::lit(2.0),
            },
        ]);

        // Collect all writes from the sequence
        let write_set = runtime.collect_effect(
            &mut model,
            &effect,
            &EntityRef::Global,
            &params,
            &mut effect_result,
        );

        // Should have 2 writes
        assert_eq!(write_set.len(), 2);

        // Not mutated yet
        assert_eq!(
            model.get_global("counter").and_then(|v| v.as_float()),
            Some(0.0)
        );

        // Apply: (0 + 10) * 2 = 20
        apply(&write_set, &mut model);

        assert_eq!(
            model.get_global("counter").and_then(|v| v.as_float()),
            Some(20.0)
        );
    }

    /// Test that spawn entities work with collect → apply
    #[test]
    fn test_collect_spawn_then_apply() {
        let mut model = Model::new();

        let mut runtime = Runtime::new();
        let mut effect_result = EffectResult::new();
        let params = ValueMap::new();

        let effect = Effect::SpawnEntity {
            kind: DefId::new("city"),
            properties: vec![
                ("name".to_string(), Expr::lit("Paris")),
                ("population".to_string(), Expr::lit(2_000_000.0)),
            ],
        };

        // Collect
        let write_set = runtime.collect_effect(
            &mut model,
            &effect,
            &EntityRef::Global,
            &params,
            &mut effect_result,
        );

        // No entities yet
        assert_eq!(model.entities().by_kind(&DefId::new("city")).count(), 0);

        // Apply
        let result = apply(&write_set, &mut model);

        // Entity created
        assert_eq!(result.spawned.len(), 1);
        let city = model.entities().get(result.spawned[0]).unwrap();
        assert_eq!(city.get("name").and_then(|v| v.as_str()), Some("Paris"));
        assert_eq!(city.get_number("population"), Some(2_000_000.0));
    }

    /// Test that events are collected in EffectResult, not WriteSet
    #[test]
    fn test_collect_emit_event() {
        let mut model = Model::new();

        let mut runtime = Runtime::new();
        let mut effect_result = EffectResult::new();
        let params = ValueMap::new();

        let effect = Effect::EmitEvent {
            event: DefId::new("battle_won"),
            target: EntityRef::Global,
            params: vec![("damage".to_string(), Expr::lit(100.0))],
        };

        let write_set = runtime.collect_effect(
            &mut model,
            &effect,
            &EntityRef::Global,
            &params,
            &mut effect_result,
        );

        // Events go to EffectResult, not WriteSet
        assert!(write_set.is_empty());
        assert_eq!(effect_result.emitted_events.len(), 1);
        assert_eq!(effect_result.emitted_events[0].0, DefId::new("battle_won"));
    }
}
