//! Deferred write semantics for parallel execution support
//!
//! This module provides types for collecting pending writes during effect execution,
//! enabling a clean separation of read/compute/write phases.
//!
//! # Overview
//!
//! Instead of mutating the `Model` directly during effect execution, handlers
//! collect `PendingWrite` operations into a `WriteSet`. These writes are then
//! applied atomically by `pulsive-hub` at the end of the tick.
//!
//! # Benefits
//!
//! - Clean separation of read/compute/write phases
//! - Enables undo/replay by storing WriteSets
//! - Foundation for parallel execution (handlers can run concurrently during read phase)
//!
//! # Architecture
//!
//! - `WriteSet` and `PendingWrite` types live in `pulsive-core`
//! - `WriteSet::apply()` is implemented in `pulsive-hub` (where the Hub owns the Model)

use crate::effect::ModifyOp;
use crate::{DefId, EntityId, Value, ValueMap};
use serde::{Deserialize, Serialize};

/// A pending write operation to be applied to the model
///
/// Each variant represents a specific mutation that will be applied atomically.
/// Values in these variants are already evaluated (no expressions).
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
/// a tick by the Hub. This enables:
/// - Deterministic parallel execution (each core produces a WriteSet)
/// - Conflict detection (compare WriteSets before merging)
/// - Journaling (store WriteSets for replay)
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

    /// Consume the WriteSet and return the underlying writes
    pub fn into_writes(self) -> Vec<PendingWrite> {
        self.writes
    }

    /// Get a reference to the underlying writes
    pub fn writes(&self) -> &[PendingWrite] {
        &self.writes
    }

    /// Clear all pending writes
    pub fn clear(&mut self) {
        self.writes.clear();
    }

    /// Merge multiple WriteSets into one
    pub fn merge(write_sets: Vec<WriteSet>) -> WriteSet {
        let mut merged = WriteSet::new();
        for ws in write_sets {
            merged.extend(ws);
        }
        merged
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_write_set_merge() {
        let mut ws1 = WriteSet::new();
        ws1.push(PendingWrite::SetGlobal {
            key: "a".to_string(),
            value: Value::Float(1.0),
        });

        let mut ws2 = WriteSet::new();
        ws2.push(PendingWrite::SetGlobal {
            key: "b".to_string(),
            value: Value::Float(2.0),
        });

        let merged = WriteSet::merge(vec![ws1, ws2]);
        assert_eq!(merged.len(), 2);
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
    fn test_write_set_result_merge() {
        let mut result1 = WriteSetResult::new();
        result1.spawned.push(EntityId(1));

        let mut result2 = WriteSetResult::new();
        result2.spawned.push(EntityId(2));
        result2.destroyed.push(EntityId(3));

        result1.merge(result2);
        assert_eq!(result1.spawned.len(), 2);
        assert_eq!(result1.destroyed.len(), 1);
    }
}
