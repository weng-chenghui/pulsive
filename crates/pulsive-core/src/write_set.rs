//! Deferred write semantics for parallel execution support
//!
//! This module provides types for collecting pending writes during effect execution,
//! enabling a clean separation of read/compute/write phases.
//!
//! # Overview
//!
//! Instead of mutating the `Model` directly during effect execution, handlers
//! collect `PendingWrite` operations into a `WriteSet`. These writes are then
//! applied atomically at the end of the tick.
//!
//! # Benefits
//!
//! - Clean separation of read/compute/write phases
//! - Enables undo/replay by storing WriteSets
//! - Foundation for parallel execution (handlers can run concurrently during read phase)
//!
//! # Example
//!
//! ```rust,ignore
//! use pulsive_core::{WriteSet, PendingWrite, Model};
//!
//! let mut write_set = WriteSet::new();
//! write_set.push(PendingWrite::SetGlobal {
//!     key: "gold".to_string(),
//!     value: Value::Float(100.0),
//! });
//!
//! // Apply all writes atomically
//! let result = write_set.apply(&mut model);
//! ```

use crate::effect::ModifyOp;
use crate::{DefId, EntityId, Value, ValueMap};
use serde::{Deserialize, Serialize};

/// A pending write operation to be applied to the model
///
/// Each variant represents a specific mutation that will be applied atomically.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PendingWrite {
    /// Set a property on an entity to a specific value
    SetProperty {
        /// The entity to modify
        entity_id: EntityId,
        /// The property key
        key: String,
        /// The value to set (already evaluated)
        value: Value,
    },

    /// Modify a numeric property on an entity
    ModifyProperty {
        /// The entity to modify
        entity_id: EntityId,
        /// The property key
        key: String,
        /// The operation to apply
        op: ModifyOp,
        /// The operand value (already evaluated to f64)
        value: f64,
    },

    /// Set a global property to a specific value
    SetGlobal {
        /// The property key
        key: String,
        /// The value to set (already evaluated)
        value: Value,
    },

    /// Modify a global numeric property
    ModifyGlobal {
        /// The property key
        key: String,
        /// The operation to apply
        op: ModifyOp,
        /// The operand value (already evaluated to f64)
        value: f64,
    },

    /// Add a flag to an entity
    AddFlag {
        /// The entity to modify
        entity_id: EntityId,
        /// The flag to add
        flag: DefId,
    },

    /// Remove a flag from an entity
    RemoveFlag {
        /// The entity to modify
        entity_id: EntityId,
        /// The flag to remove
        flag: DefId,
    },

    /// Spawn a new entity
    SpawnEntity {
        /// The kind of entity to create
        kind: DefId,
        /// Initial properties (already evaluated)
        properties: ValueMap,
    },

    /// Destroy an entity
    DestroyEntity {
        /// The entity to destroy
        id: EntityId,
    },
}

/// Result of applying a WriteSet to a model
#[derive(Debug, Clone, Default)]
pub struct WriteSetResult {
    /// Entity IDs that were spawned
    pub spawned: Vec<EntityId>,
    /// Entity IDs that were destroyed
    pub destroyed: Vec<EntityId>,
}

impl WriteSetResult {
    /// Create a new empty result
    pub fn new() -> Self {
        Self::default()
    }

    /// Merge another result into this one
    pub fn merge(&mut self, other: WriteSetResult) {
        self.spawned.extend(other.spawned);
        self.destroyed.extend(other.destroyed);
    }
}

/// A collection of pending writes to be applied atomically
///
/// WriteSets are collected during effect execution and applied at the end of
/// a tick or message processing cycle.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WriteSet {
    /// The pending writes in order
    writes: Vec<PendingWrite>,
}

impl WriteSet {
    /// Create a new empty WriteSet
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a pending write to the set
    pub fn push(&mut self, write: PendingWrite) {
        self.writes.push(write);
    }

    /// Extend this WriteSet with writes from another
    pub fn extend(&mut self, other: WriteSet) {
        self.writes.extend(other.writes);
    }

    /// Get the number of pending writes
    pub fn len(&self) -> usize {
        self.writes.len()
    }

    /// Check if the WriteSet is empty
    pub fn is_empty(&self) -> bool {
        self.writes.is_empty()
    }

    /// Get an iterator over the pending writes
    pub fn iter(&self) -> impl Iterator<Item = &PendingWrite> {
        self.writes.iter()
    }

    /// Apply all pending writes to the model atomically
    ///
    /// Returns information about spawned and destroyed entities.
    pub fn apply(&self, model: &mut crate::Model) -> WriteSetResult {
        let mut result = WriteSetResult::new();

        for write in &self.writes {
            match write {
                PendingWrite::SetProperty {
                    entity_id,
                    key,
                    value,
                } => {
                    if let Some(entity) = model.entities.get_mut(*entity_id) {
                        entity.set(key.clone(), value.clone());
                    }
                }

                PendingWrite::ModifyProperty {
                    entity_id,
                    key,
                    op,
                    value,
                } => {
                    if let Some(entity) = model.entities.get_mut(*entity_id) {
                        let current = entity.get_number(key).unwrap_or(0.0);
                        let new_value = op.apply(current, *value);
                        entity.set(key.clone(), new_value);
                    }
                }

                PendingWrite::SetGlobal { key, value } => {
                    model.globals.insert(key.clone(), value.clone());
                }

                PendingWrite::ModifyGlobal { key, op, value } => {
                    let current = model
                        .globals
                        .get(key)
                        .and_then(|v| v.as_float())
                        .unwrap_or(0.0);
                    let new_value = op.apply(current, *value);
                    model.globals.insert(key.clone(), Value::Float(new_value));
                }

                PendingWrite::AddFlag { entity_id, flag } => {
                    if let Some(entity) = model.entities.get_mut(*entity_id) {
                        entity.add_flag(flag.clone());
                    }
                }

                PendingWrite::RemoveFlag { entity_id, flag } => {
                    if let Some(entity) = model.entities.get_mut(*entity_id) {
                        entity.remove_flag(flag);
                    }
                }

                PendingWrite::SpawnEntity { kind, properties } => {
                    let entity = model.entities.create(kind.clone());
                    let entity_id = entity.id;

                    // Set initial properties
                    for (key, value) in properties {
                        entity.set(key.clone(), value.clone());
                    }

                    result.spawned.push(entity_id);
                }

                PendingWrite::DestroyEntity { id } => {
                    model.entities.remove(*id);
                    result.destroyed.push(*id);
                }
            }
        }

        result
    }

    /// Clear all pending writes
    pub fn clear(&mut self) {
        self.writes.clear();
    }

    /// Consume the WriteSet and return the underlying writes
    pub fn into_writes(self) -> Vec<PendingWrite> {
        self.writes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Model;

    #[test]
    fn test_write_set_empty() {
        let write_set = WriteSet::new();
        assert!(write_set.is_empty());
        assert_eq!(write_set.len(), 0);
    }

    #[test]
    fn test_write_set_push() {
        let mut write_set = WriteSet::new();
        write_set.push(PendingWrite::SetGlobal {
            key: "test".to_string(),
            value: Value::Float(42.0),
        });
        assert_eq!(write_set.len(), 1);
        assert!(!write_set.is_empty());
    }

    #[test]
    fn test_write_set_extend() {
        let mut write_set1 = WriteSet::new();
        write_set1.push(PendingWrite::SetGlobal {
            key: "a".to_string(),
            value: Value::Float(1.0),
        });

        let mut write_set2 = WriteSet::new();
        write_set2.push(PendingWrite::SetGlobal {
            key: "b".to_string(),
            value: Value::Float(2.0),
        });

        write_set1.extend(write_set2);
        assert_eq!(write_set1.len(), 2);
    }

    #[test]
    fn test_apply_set_global() {
        let mut model = Model::new();
        let mut write_set = WriteSet::new();

        write_set.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(100.0),
        });

        write_set.apply(&mut model);

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

        write_set.apply(&mut model);

        assert_eq!(
            model.get_global("gold").and_then(|v| v.as_float()),
            Some(150.0)
        );
    }

    #[test]
    fn test_apply_entity_property() {
        let mut model = Model::new();
        let entity = model.entities.create("nation");
        entity.set("gold", 100.0f64);
        let entity_id = entity.id;

        let mut write_set = WriteSet::new();
        write_set.push(PendingWrite::SetProperty {
            entity_id,
            key: "gold".to_string(),
            value: Value::Float(200.0),
        });

        write_set.apply(&mut model);

        assert_eq!(
            model
                .entities
                .get(entity_id)
                .and_then(|e| e.get_number("gold")),
            Some(200.0)
        );
    }

    #[test]
    fn test_apply_modify_entity_property() {
        let mut model = Model::new();
        let entity = model.entities.create("nation");
        entity.set("gold", 100.0f64);
        let entity_id = entity.id;

        let mut write_set = WriteSet::new();
        write_set.push(PendingWrite::ModifyProperty {
            entity_id,
            key: "gold".to_string(),
            op: ModifyOp::Mul,
            value: 2.0,
        });

        write_set.apply(&mut model);

        assert_eq!(
            model
                .entities
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

        let result = write_set.apply(&mut model);

        assert_eq!(result.spawned.len(), 1);
        let entity_id = result.spawned[0];
        let entity = model.entities.get(entity_id).unwrap();
        assert_eq!(entity.get("name").and_then(|v| v.as_str()), Some("France"));
        assert_eq!(entity.get_number("gold"), Some(100.0));
    }

    #[test]
    fn test_apply_destroy_entity() {
        let mut model = Model::new();
        let entity = model.entities.create("nation");
        let entity_id = entity.id;

        assert!(model.entities.get(entity_id).is_some());

        let mut write_set = WriteSet::new();
        write_set.push(PendingWrite::DestroyEntity { id: entity_id });

        let result = write_set.apply(&mut model);

        assert_eq!(result.destroyed.len(), 1);
        assert!(model.entities.get(entity_id).is_none());
    }

    #[test]
    fn test_apply_flags() {
        let mut model = Model::new();
        let entity = model.entities.create("nation");
        let entity_id = entity.id;

        let mut write_set = WriteSet::new();
        write_set.push(PendingWrite::AddFlag {
            entity_id,
            flag: DefId::new("at_war"),
        });

        write_set.apply(&mut model);

        let entity = model.entities.get(entity_id).unwrap();
        assert!(entity.has_flag(&DefId::new("at_war")));

        // Remove the flag
        let mut write_set = WriteSet::new();
        write_set.push(PendingWrite::RemoveFlag {
            entity_id,
            flag: DefId::new("at_war"),
        });

        write_set.apply(&mut model);

        let entity = model.entities.get(entity_id).unwrap();
        assert!(!entity.has_flag(&DefId::new("at_war")));
    }

    #[test]
    fn test_write_set_serialization() {
        let mut write_set = WriteSet::new();
        write_set.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(100.0),
        });
        write_set.push(PendingWrite::SpawnEntity {
            kind: DefId::new("nation"),
            properties: ValueMap::new(),
        });

        // Test that it can be serialized and deserialized using RON
        let serialized = ron::to_string(&write_set).expect("serialize");
        let deserialized: WriteSet = ron::from_str(&serialized).expect("deserialize");

        assert_eq!(deserialized.len(), 2);
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

        write_set.apply(&mut model);

        // Should be (0 + 10) * 2 = 20
        assert_eq!(
            model.get_global("counter").and_then(|v| v.as_float()),
            Some(20.0)
        );
    }
}
