//! Conflict detection for parallel WriteSet merging
//!
//! When multiple cores execute in parallel, they may produce conflicting writes.
//! This module detects such conflicts before merging WriteSets.
//!
//! # Conflict Types
//!
//! - **Write-Write**: Two cores wrote to the same (entity, property) or global
//! - **Read-Write** (optional): Core A read what Core B wrote
//!
//! # Algorithm
//!
//! Conflict detection is O(n) where n = total writes across all WriteSets:
//! 1. Build a map from write targets to (core_id, write) pairs
//! 2. Any target with multiple entries is a conflict

use crate::CoreId;
use pulsive_core::{DefId, EntityId, PendingWrite, WriteSet};
use std::collections::HashMap;

/// The target of a write operation - used as the key for conflict detection
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WriteTarget {
    /// Property on a specific entity
    EntityProperty {
        entity_id: EntityId,
        property: String,
    },
    /// Flag on a specific entity
    EntityFlag { entity_id: EntityId, flag: DefId },
    /// Global property
    GlobalProperty { property: String },
    /// Entity spawn (conflicts if same kind spawned by multiple cores - usually OK)
    SpawnEntity { kind: DefId },
    /// Entity destruction (conflicts if same entity destroyed by multiple cores)
    DestroyEntity { entity_id: EntityId },
}

impl WriteTarget {
    /// Extract the target from a PendingWrite
    pub fn from_pending_write(write: &PendingWrite) -> Self {
        match write {
            PendingWrite::SetProperty { entity_id, key, .. } => WriteTarget::EntityProperty {
                entity_id: *entity_id,
                property: key.clone(),
            },
            PendingWrite::ModifyProperty { entity_id, key, .. } => WriteTarget::EntityProperty {
                entity_id: *entity_id,
                property: key.clone(),
            },
            PendingWrite::SetGlobal { key, .. } => WriteTarget::GlobalProperty {
                property: key.clone(),
            },
            PendingWrite::ModifyGlobal { key, .. } => WriteTarget::GlobalProperty {
                property: key.clone(),
            },
            PendingWrite::AddFlag { entity_id, flag } => WriteTarget::EntityFlag {
                entity_id: *entity_id,
                flag: flag.clone(),
            },
            PendingWrite::RemoveFlag { entity_id, flag } => WriteTarget::EntityFlag {
                entity_id: *entity_id,
                flag: flag.clone(),
            },
            PendingWrite::SpawnEntity { kind, .. } => {
                WriteTarget::SpawnEntity { kind: kind.clone() }
            }
            PendingWrite::DestroyEntity { id } => WriteTarget::DestroyEntity { entity_id: *id },
        }
    }
}

/// The kind of conflict detected
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictKind {
    /// Two cores wrote to the same entity property
    EntityPropertyWriteWrite {
        entity_id: EntityId,
        property: String,
        core_a: CoreId,
        core_b: CoreId,
    },
    /// Two cores modified the same entity flag
    EntityFlagWriteWrite {
        entity_id: EntityId,
        flag: DefId,
        core_a: CoreId,
        core_b: CoreId,
    },
    /// Two cores wrote to the same global property
    GlobalPropertyWriteWrite {
        property: String,
        core_a: CoreId,
        core_b: CoreId,
    },
    /// Two cores tried to destroy the same entity
    DestroyEntityWriteWrite {
        entity_id: EntityId,
        core_a: CoreId,
        core_b: CoreId,
    },
}

impl std::fmt::Display for ConflictKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConflictKind::EntityPropertyWriteWrite {
                entity_id,
                property,
                core_a,
                core_b,
            } => {
                write!(
                    f,
                    "Write-write conflict on entity {:?} property '{}' between {} and {}",
                    entity_id, property, core_a, core_b
                )
            }
            ConflictKind::EntityFlagWriteWrite {
                entity_id,
                flag,
                core_a,
                core_b,
            } => {
                write!(
                    f,
                    "Write-write conflict on entity {:?} flag '{}' between {} and {}",
                    entity_id, flag, core_a, core_b
                )
            }
            ConflictKind::GlobalPropertyWriteWrite {
                property,
                core_a,
                core_b,
            } => {
                write!(
                    f,
                    "Write-write conflict on global '{}' between {} and {}",
                    property, core_a, core_b
                )
            }
            ConflictKind::DestroyEntityWriteWrite {
                entity_id,
                core_a,
                core_b,
            } => {
                write!(
                    f,
                    "Write-write conflict: entity {:?} destroyed by both {} and {}",
                    entity_id, core_a, core_b
                )
            }
        }
    }
}

/// A detected conflict with diagnostic information
#[derive(Debug, Clone)]
pub struct Conflict {
    /// The kind of conflict
    pub kind: ConflictKind,
    /// The conflicting writes (for debugging/resolution)
    pub writes: Vec<(CoreId, PendingWrite)>,
}

impl Conflict {
    /// Create a new conflict
    pub fn new(kind: ConflictKind, writes: Vec<(CoreId, PendingWrite)>) -> Self {
        Self { kind, writes }
    }
}

impl std::fmt::Display for Conflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)
    }
}

/// Result of conflict detection
#[derive(Debug, Clone, Default)]
pub struct ConflictReport {
    /// All detected conflicts
    pub conflicts: Vec<Conflict>,
}

impl ConflictReport {
    /// Create an empty report
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if there are any conflicts
    pub fn has_conflicts(&self) -> bool {
        !self.conflicts.is_empty()
    }

    /// Get the number of conflicts
    pub fn len(&self) -> usize {
        self.conflicts.len()
    }

    /// Check if the report is empty
    pub fn is_empty(&self) -> bool {
        self.conflicts.is_empty()
    }

    /// Iterate over conflicts
    pub fn iter(&self) -> impl Iterator<Item = &Conflict> {
        self.conflicts.iter()
    }
}

/// Detect write-write conflicts across multiple WriteSets from different cores
///
/// # Algorithm
///
/// 1. Build a HashMap from `WriteTarget` to `Vec<(CoreId, PendingWrite)>`
/// 2. Any target with more than one entry represents a conflict
///
/// # Complexity
///
/// O(n) where n = total number of writes across all WriteSets
///
/// # Arguments
///
/// * `write_sets` - Slice of (CoreId, WriteSet) pairs from each core
///
/// # Returns
///
/// A `ConflictReport` containing all detected conflicts
pub fn detect_conflicts(write_sets: &[(CoreId, WriteSet)]) -> ConflictReport {
    let mut write_map: HashMap<WriteTarget, Vec<(CoreId, PendingWrite)>> = HashMap::new();

    // Phase 1: Collect all writes by target
    for (core_id, ws) in write_sets {
        for write in ws.iter() {
            let target = WriteTarget::from_pending_write(write);
            write_map
                .entry(target)
                .or_default()
                .push((*core_id, write.clone()));
        }
    }

    // Phase 2: Find conflicts (targets with multiple writers)
    let mut report = ConflictReport::new();

    for (target, writes) in write_map {
        if writes.len() > 1 {
            // We have a conflict - multiple cores wrote to the same target
            let conflict = create_conflict(&target, writes);
            report.conflicts.push(conflict);
        }
    }

    report
}

/// Create a Conflict from a target and its conflicting writes
fn create_conflict(target: &WriteTarget, writes: Vec<(CoreId, PendingWrite)>) -> Conflict {
    // Get the first two cores for the conflict kind
    // (there may be more than 2 cores in conflict)
    let core_a = writes[0].0;
    let core_b = writes[1].0;

    let kind = match target {
        WriteTarget::EntityProperty {
            entity_id,
            property,
        } => ConflictKind::EntityPropertyWriteWrite {
            entity_id: *entity_id,
            property: property.clone(),
            core_a,
            core_b,
        },
        WriteTarget::EntityFlag { entity_id, flag } => ConflictKind::EntityFlagWriteWrite {
            entity_id: *entity_id,
            flag: flag.clone(),
            core_a,
            core_b,
        },
        WriteTarget::GlobalProperty { property } => ConflictKind::GlobalPropertyWriteWrite {
            property: property.clone(),
            core_a,
            core_b,
        },
        WriteTarget::SpawnEntity { kind: _ } => {
            // Spawn conflicts are unusual - multiple cores spawning same kind is often OK
            // For now, we don't generate conflicts for spawns
            // This branch won't be reached due to how we filter, but we handle it gracefully
            ConflictKind::GlobalPropertyWriteWrite {
                property: "spawn".to_string(),
                core_a,
                core_b,
            }
        }
        WriteTarget::DestroyEntity { entity_id } => ConflictKind::DestroyEntityWriteWrite {
            entity_id: *entity_id,
            core_a,
            core_b,
        },
    };

    Conflict::new(kind, writes)
}

/// Detect conflicts with an option to exclude certain target types
///
/// This is useful for cases where spawn conflicts are acceptable
/// (e.g., multiple cores spawning entities of the same kind is often fine)
pub fn detect_conflicts_filtered<F>(write_sets: &[(CoreId, WriteSet)], filter: F) -> ConflictReport
where
    F: Fn(&WriteTarget) -> bool,
{
    let mut write_map: HashMap<WriteTarget, Vec<(CoreId, PendingWrite)>> = HashMap::new();

    for (core_id, ws) in write_sets {
        for write in ws.iter() {
            let target = WriteTarget::from_pending_write(write);
            if filter(&target) {
                write_map
                    .entry(target)
                    .or_default()
                    .push((*core_id, write.clone()));
            }
        }
    }

    let mut report = ConflictReport::new();

    for (target, writes) in write_map {
        if writes.len() > 1 {
            let conflict = create_conflict(&target, writes);
            report.conflicts.push(conflict);
        }
    }

    report
}

/// Default filter that excludes spawn conflicts (which are usually acceptable)
pub fn default_conflict_filter(target: &WriteTarget) -> bool {
    !matches!(target, WriteTarget::SpawnEntity { .. })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsive_core::{ModifyOp, Value};

    #[test]
    fn test_no_conflicts_single_core() {
        let core_id = CoreId(0);
        let mut ws = WriteSet::new();
        ws.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(100.0),
        });

        let report = detect_conflicts(&[(core_id, ws)]);
        assert!(!report.has_conflicts());
    }

    #[test]
    fn test_no_conflicts_disjoint_writes() {
        let mut ws1 = WriteSet::new();
        ws1.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(100.0),
        });

        let mut ws2 = WriteSet::new();
        ws2.push(PendingWrite::SetGlobal {
            key: "silver".to_string(),
            value: Value::Float(200.0),
        });

        let report = detect_conflicts(&[(CoreId(0), ws1), (CoreId(1), ws2)]);
        assert!(!report.has_conflicts());
    }

    #[test]
    fn test_global_write_write_conflict() {
        let mut ws1 = WriteSet::new();
        ws1.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(100.0),
        });

        let mut ws2 = WriteSet::new();
        ws2.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(200.0),
        });

        let report = detect_conflicts(&[(CoreId(0), ws1), (CoreId(1), ws2)]);
        assert!(report.has_conflicts());
        assert_eq!(report.len(), 1);

        match &report.conflicts[0].kind {
            ConflictKind::GlobalPropertyWriteWrite {
                property,
                core_a,
                core_b,
            } => {
                assert_eq!(property, "gold");
                assert_eq!(*core_a, CoreId(0));
                assert_eq!(*core_b, CoreId(1));
            }
            _ => panic!("Expected GlobalPropertyWriteWrite"),
        }
    }

    #[test]
    fn test_entity_property_write_write_conflict() {
        let entity_id = EntityId::new(42);

        let mut ws1 = WriteSet::new();
        ws1.push(PendingWrite::SetProperty {
            entity_id,
            key: "health".to_string(),
            value: Value::Float(100.0),
        });

        let mut ws2 = WriteSet::new();
        ws2.push(PendingWrite::ModifyProperty {
            entity_id,
            key: "health".to_string(),
            op: ModifyOp::Add,
            value: -10.0,
        });

        let report = detect_conflicts(&[(CoreId(0), ws1), (CoreId(1), ws2)]);
        assert!(report.has_conflicts());
        assert_eq!(report.len(), 1);

        match &report.conflicts[0].kind {
            ConflictKind::EntityPropertyWriteWrite {
                entity_id: eid,
                property,
                ..
            } => {
                assert_eq!(*eid, entity_id);
                assert_eq!(property, "health");
            }
            _ => panic!("Expected EntityPropertyWriteWrite"),
        }
    }

    #[test]
    fn test_entity_flag_write_write_conflict() {
        let entity_id = EntityId::new(42);
        let flag = DefId::new("poisoned");

        let mut ws1 = WriteSet::new();
        ws1.push(PendingWrite::AddFlag {
            entity_id,
            flag: flag.clone(),
        });

        let mut ws2 = WriteSet::new();
        ws2.push(PendingWrite::RemoveFlag {
            entity_id,
            flag: flag.clone(),
        });

        let report = detect_conflicts(&[(CoreId(0), ws1), (CoreId(1), ws2)]);
        assert!(report.has_conflicts());
        assert_eq!(report.len(), 1);

        match &report.conflicts[0].kind {
            ConflictKind::EntityFlagWriteWrite {
                entity_id: eid,
                flag: f,
                ..
            } => {
                assert_eq!(*eid, entity_id);
                assert_eq!(*f, flag);
            }
            _ => panic!("Expected EntityFlagWriteWrite"),
        }
    }

    #[test]
    fn test_destroy_entity_write_write_conflict() {
        let entity_id = EntityId::new(42);

        let mut ws1 = WriteSet::new();
        ws1.push(PendingWrite::DestroyEntity { id: entity_id });

        let mut ws2 = WriteSet::new();
        ws2.push(PendingWrite::DestroyEntity { id: entity_id });

        let report = detect_conflicts(&[(CoreId(0), ws1), (CoreId(1), ws2)]);
        assert!(report.has_conflicts());
        assert_eq!(report.len(), 1);

        match &report.conflicts[0].kind {
            ConflictKind::DestroyEntityWriteWrite { entity_id: eid, .. } => {
                assert_eq!(*eid, entity_id);
            }
            _ => panic!("Expected DestroyEntityWriteWrite"),
        }
    }

    #[test]
    fn test_multiple_conflicts() {
        let entity_id = EntityId::new(42);

        let mut ws1 = WriteSet::new();
        ws1.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(100.0),
        });
        ws1.push(PendingWrite::SetProperty {
            entity_id,
            key: "health".to_string(),
            value: Value::Float(100.0),
        });

        let mut ws2 = WriteSet::new();
        ws2.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(200.0),
        });
        ws2.push(PendingWrite::SetProperty {
            entity_id,
            key: "health".to_string(),
            value: Value::Float(50.0),
        });

        let report = detect_conflicts(&[(CoreId(0), ws1), (CoreId(1), ws2)]);
        assert!(report.has_conflicts());
        assert_eq!(report.len(), 2);
    }

    #[test]
    fn test_three_way_conflict() {
        let mut ws1 = WriteSet::new();
        ws1.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(100.0),
        });

        let mut ws2 = WriteSet::new();
        ws2.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(200.0),
        });

        let mut ws3 = WriteSet::new();
        ws3.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(300.0),
        });

        let report = detect_conflicts(&[(CoreId(0), ws1), (CoreId(1), ws2), (CoreId(2), ws3)]);
        assert!(report.has_conflicts());
        assert_eq!(report.len(), 1);
        // All 3 writes should be in the conflict
        assert_eq!(report.conflicts[0].writes.len(), 3);
    }

    #[test]
    fn test_spawn_not_filtered_by_default_detect() {
        let kind = DefId::new("unit");

        let mut ws1 = WriteSet::new();
        ws1.push(PendingWrite::SpawnEntity {
            kind: kind.clone(),
            properties: Default::default(),
        });

        let mut ws2 = WriteSet::new();
        ws2.push(PendingWrite::SpawnEntity {
            kind: kind.clone(),
            properties: Default::default(),
        });

        // Regular detect_conflicts includes spawns
        let report = detect_conflicts(&[(CoreId(0), ws1.clone()), (CoreId(1), ws2.clone())]);
        assert!(report.has_conflicts());

        // Filtered version excludes spawns
        let report_filtered = detect_conflicts_filtered(
            &[(CoreId(0), ws1), (CoreId(1), ws2)],
            default_conflict_filter,
        );
        assert!(!report_filtered.has_conflicts());
    }

    #[test]
    fn test_conflict_display() {
        let conflict = Conflict::new(
            ConflictKind::GlobalPropertyWriteWrite {
                property: "gold".to_string(),
                core_a: CoreId(0),
                core_b: CoreId(1),
            },
            vec![],
        );

        let display = format!("{}", conflict);
        assert!(display.contains("gold"));
        assert!(display.contains("Core(0)"));
        assert!(display.contains("Core(1)"));
    }

    #[test]
    fn test_write_target_extraction() {
        let entity_id = EntityId::new(42);

        // SetProperty
        let write = PendingWrite::SetProperty {
            entity_id,
            key: "health".to_string(),
            value: Value::Float(100.0),
        };
        let target = WriteTarget::from_pending_write(&write);
        assert!(matches!(
            target,
            WriteTarget::EntityProperty {
                entity_id: _,
                property
            } if property == "health"
        ));

        // ModifyGlobal
        let write = PendingWrite::ModifyGlobal {
            key: "gold".to_string(),
            op: ModifyOp::Add,
            value: 10.0,
        };
        let target = WriteTarget::from_pending_write(&write);
        assert!(matches!(
            target,
            WriteTarget::GlobalProperty { property } if property == "gold"
        ));

        // DestroyEntity
        let write = PendingWrite::DestroyEntity { id: entity_id };
        let target = WriteTarget::from_pending_write(&write);
        assert!(matches!(
            target,
            WriteTarget::DestroyEntity { entity_id: eid } if eid == entity_id
        ));
    }
}
