//! Conflict detection for parallel WriteSet merging
//!
//! When multiple cores execute in parallel, they may produce conflicting writes.
//! This module detects such conflicts before merging WriteSets.
//!
//! # Conflict Types
//!
//! - **Write-Write**: Two different cores wrote to the same (entity, property) or global
//! - **Read-Write** (optional, future): Core A read what Core B wrote
//!
//! # Algorithm
//!
//! Conflict detection is O(n) where n = total writes across all WriteSets:
//! 1. Build a map from write targets to (core_id, write) pairs
//! 2. Any target with writes from multiple distinct cores is a conflict
//!
//! Note: Multiple writes from the *same* core to the same target are NOT conflicts -
//! they are simply a sequence of operations that the core will order internally.
//!
//! # Spawn Conflicts
//!
//! By default, `detect_conflicts` reports ALL conflicts including spawn conflicts.
//! Spawn conflicts (multiple cores spawning entities of the same kind) are often
//! acceptable because spawns are independent operations - each core creates its own
//! new entity. Use `detect_conflicts_filtered` with `default_conflict_filter` to
//! exclude spawn conflicts if they are not relevant to your use case.

use crate::CoreId;
use pulsive_core::{DefId, EntityId, PendingWrite, WriteSet};
use std::collections::{HashMap, HashSet};

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
    /// Two cores spawned entities of the same kind
    ///
    /// This is often acceptable (multiple spawns are independent) but is
    /// detected for cases where spawn order matters or needs coordination.
    /// Use `default_conflict_filter` to exclude spawn conflicts if desired.
    SpawnEntityWriteWrite {
        kind: DefId,
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
                    "Write-write conflict on entity {} property '{}' between {} and {}",
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
                    "Write-write conflict on entity {} flag '{}' between {} and {}",
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
            ConflictKind::SpawnEntityWriteWrite {
                kind,
                core_a,
                core_b,
            } => {
                write!(
                    f,
                    "Write-write conflict: entity kind '{}' spawned by both {} and {}",
                    kind, core_a, core_b
                )
            }
            ConflictKind::DestroyEntityWriteWrite {
                entity_id,
                core_a,
                core_b,
            } => {
                write!(
                    f,
                    "Write-write conflict: entity {} destroyed by both {} and {}",
                    entity_id, core_a, core_b
                )
            }
        }
    }
}

/// A detected conflict with diagnostic information
///
/// Contains all information needed for conflict resolution:
/// - `kind`: The type of conflict with representative cores (first two by ID order)
/// - `cores`: All distinct cores involved (sorted by ID for deterministic output)
/// - `writes`: All conflicting writes for debugging/resolution
#[derive(Debug, Clone, PartialEq)]
pub struct Conflict {
    /// The kind of conflict (includes first two cores for quick identification)
    pub kind: ConflictKind,
    /// All distinct cores involved in this conflict (sorted by CoreId for determinism)
    pub cores: Vec<CoreId>,
    /// All conflicting writes from all cores (for debugging/resolution)
    pub writes: Vec<(CoreId, PendingWrite)>,
}

impl Conflict {
    /// Create a new conflict
    pub fn new(
        kind: ConflictKind,
        cores: Vec<CoreId>,
        writes: Vec<(CoreId, PendingWrite)>,
    ) -> Self {
        Self {
            kind,
            cores,
            writes,
        }
    }

    /// Get the number of cores involved in this conflict
    pub fn core_count(&self) -> usize {
        self.cores.len()
    }
}

impl std::fmt::Display for Conflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)
    }
}

/// Result of conflict detection
#[derive(Debug, Clone, Default, PartialEq)]
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
/// 2. Any target with writes from multiple distinct cores is a conflict
///
/// Note: Multiple writes from the *same* core to the same target are NOT conflicts.
/// They are simply a sequence of operations that the core orders internally.
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

    // Phase 2: Find conflicts (targets with writes from multiple distinct cores)
    let mut report = ConflictReport::new();

    for (target, writes) in write_map {
        // Collect distinct core IDs
        let distinct_cores: HashSet<CoreId> = writes.iter().map(|(c, _)| *c).collect();

        // Only a conflict if at least two distinct cores wrote to the same target
        if distinct_cores.len() > 1 {
            let conflict = create_conflict(&target, writes, distinct_cores);
            report.conflicts.push(conflict);
        }
    }

    report
}

/// Create a Conflict from a target, its conflicting writes, and the set of distinct cores
///
/// # Precondition
///
/// `distinct_cores` must contain at least 2 elements. This is enforced by a debug_assert.
fn create_conflict(
    target: &WriteTarget,
    writes: Vec<(CoreId, PendingWrite)>,
    distinct_cores: HashSet<CoreId>,
) -> Conflict {
    debug_assert!(
        distinct_cores.len() >= 2,
        "create_conflict requires at least 2 distinct cores, got {}",
        distinct_cores.len()
    );

    // Sort cores by ID for deterministic ordering in output
    let mut sorted_cores: Vec<CoreId> = distinct_cores.into_iter().collect();
    sorted_cores.sort_by_key(|c| c.0);

    // Use first two sorted cores for the ConflictKind
    let core_a = sorted_cores[0];
    let core_b = sorted_cores[1];

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
        WriteTarget::SpawnEntity { kind } => ConflictKind::SpawnEntityWriteWrite {
            kind: kind.clone(),
            core_a,
            core_b,
        },
        WriteTarget::DestroyEntity { entity_id } => ConflictKind::DestroyEntityWriteWrite {
            entity_id: *entity_id,
            core_a,
            core_b,
        },
    };

    Conflict::new(kind, sorted_cores, writes)
}

/// Detect conflicts with an option to exclude certain target types
///
/// This is useful for cases where spawn conflicts are acceptable
/// (e.g., multiple cores spawning entities of the same kind is often fine).
/// Use `default_conflict_filter` to exclude spawn conflicts.
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
        // Collect distinct core IDs
        let distinct_cores: HashSet<CoreId> = writes.iter().map(|(c, _)| *c).collect();

        // Only a conflict if at least two distinct cores wrote to the same target
        if distinct_cores.len() > 1 {
            let conflict = create_conflict(&target, writes, distinct_cores);
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
                // Cores are now sorted, so ordering is deterministic
                assert_eq!(*core_a, CoreId(0));
                assert_eq!(*core_b, CoreId(1));
            }
            _ => panic!("Expected GlobalPropertyWriteWrite"),
        }

        // Also check the cores field
        assert_eq!(report.conflicts[0].cores, vec![CoreId(0), CoreId(1)]);
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
    fn test_spawn_conflict_detected() {
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
        assert_eq!(report.len(), 1);

        // Verify it's a SpawnEntityWriteWrite conflict
        match &report.conflicts[0].kind {
            ConflictKind::SpawnEntityWriteWrite { kind: k, .. } => {
                assert_eq!(k.as_str(), "unit");
            }
            _ => panic!("Expected SpawnEntityWriteWrite"),
        }

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
            vec![CoreId(0), CoreId(1)],
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

    // ========================================================================
    // Edge case tests for same-core multiple writes
    // ========================================================================

    #[test]
    fn test_same_core_multiple_writes_no_conflict() {
        // Multiple writes from the SAME core to the same target should NOT be a conflict
        let mut ws = WriteSet::new();
        ws.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(100.0),
        });
        ws.push(PendingWrite::ModifyGlobal {
            key: "gold".to_string(),
            op: ModifyOp::Add,
            value: 50.0,
        });
        ws.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(200.0),
        });

        // All from same core - no conflict
        let report = detect_conflicts(&[(CoreId(0), ws)]);
        assert!(!report.has_conflicts());
    }

    #[test]
    fn test_same_core_multiple_writes_entity_property_no_conflict() {
        // Multiple writes from the SAME core to the same entity property
        let entity_id = EntityId::new(42);

        let mut ws = WriteSet::new();
        ws.push(PendingWrite::SetProperty {
            entity_id,
            key: "health".to_string(),
            value: Value::Float(100.0),
        });
        ws.push(PendingWrite::ModifyProperty {
            entity_id,
            key: "health".to_string(),
            op: ModifyOp::Add,
            value: -10.0,
        });

        // All from same core - no conflict
        let report = detect_conflicts(&[(CoreId(0), ws)]);
        assert!(!report.has_conflicts());
    }

    #[test]
    fn test_mixed_same_core_and_different_cores() {
        // Core 0 writes twice, Core 1 writes once - should be a conflict
        let mut ws0 = WriteSet::new();
        ws0.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(100.0),
        });
        ws0.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(150.0),
        });

        let mut ws1 = WriteSet::new();
        ws1.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(200.0),
        });

        let report = detect_conflicts(&[(CoreId(0), ws0), (CoreId(1), ws1)]);
        assert!(report.has_conflicts());
        assert_eq!(report.len(), 1);

        // All 3 writes should be in the conflict for diagnostics
        assert_eq!(report.conflicts[0].writes.len(), 3);

        // The conflict kind should mention both distinct cores (sorted order)
        match &report.conflicts[0].kind {
            ConflictKind::GlobalPropertyWriteWrite {
                property,
                core_a,
                core_b,
            } => {
                assert_eq!(property, "gold");
                // Cores are now sorted deterministically
                assert_eq!(*core_a, CoreId(0));
                assert_eq!(*core_b, CoreId(1));
            }
            _ => panic!("Expected GlobalPropertyWriteWrite"),
        }

        // Check all cores are in the cores field
        assert_eq!(report.conflicts[0].cores, vec![CoreId(0), CoreId(1)]);
    }

    #[test]
    fn test_multiple_cores_with_duplicates() {
        // Core 0 writes twice, Core 1 writes twice, Core 2 writes once
        let mut ws0 = WriteSet::new();
        ws0.push(PendingWrite::SetGlobal {
            key: "score".to_string(),
            value: Value::Float(10.0),
        });
        ws0.push(PendingWrite::SetGlobal {
            key: "score".to_string(),
            value: Value::Float(20.0),
        });

        let mut ws1 = WriteSet::new();
        ws1.push(PendingWrite::SetGlobal {
            key: "score".to_string(),
            value: Value::Float(30.0),
        });
        ws1.push(PendingWrite::SetGlobal {
            key: "score".to_string(),
            value: Value::Float(40.0),
        });

        let mut ws2 = WriteSet::new();
        ws2.push(PendingWrite::SetGlobal {
            key: "score".to_string(),
            value: Value::Float(50.0),
        });

        let report = detect_conflicts(&[(CoreId(0), ws0), (CoreId(1), ws1), (CoreId(2), ws2)]);
        assert!(report.has_conflicts());
        assert_eq!(report.len(), 1);

        // All 5 writes should be in the conflict for diagnostics
        assert_eq!(report.conflicts[0].writes.len(), 5);
    }

    #[test]
    fn test_spawn_conflict_display() {
        let conflict = Conflict::new(
            ConflictKind::SpawnEntityWriteWrite {
                kind: DefId::new("unit"),
                core_a: CoreId(0),
                core_b: CoreId(1),
            },
            vec![CoreId(0), CoreId(1)],
            vec![],
        );

        let display = format!("{}", conflict);
        assert!(display.contains("unit"));
        assert!(display.contains("spawned"));
        assert!(display.contains("Core(0)"));
        assert!(display.contains("Core(1)"));
    }

    // ========================================================================
    // Tests for custom filters
    // ========================================================================

    #[test]
    fn test_custom_filter_exclude_globals() {
        // Custom filter that excludes global property conflicts
        let exclude_globals =
            |target: &WriteTarget| !matches!(target, WriteTarget::GlobalProperty { .. });

        let entity_id = EntityId::new(42);

        // Create writes: one global conflict, one entity property conflict
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

        // Without filter: 2 conflicts (global + entity property)
        let report_unfiltered =
            detect_conflicts(&[(CoreId(0), ws1.clone()), (CoreId(1), ws2.clone())]);
        assert_eq!(report_unfiltered.len(), 2);

        // With custom filter: only entity property conflict
        let report_filtered =
            detect_conflicts_filtered(&[(CoreId(0), ws1), (CoreId(1), ws2)], exclude_globals);
        assert_eq!(report_filtered.len(), 1);
        assert!(matches!(
            &report_filtered.conflicts[0].kind,
            ConflictKind::EntityPropertyWriteWrite { .. }
        ));
    }

    #[test]
    fn test_custom_filter_only_destroys() {
        // Custom filter that only includes destroy conflicts
        let only_destroys =
            |target: &WriteTarget| matches!(target, WriteTarget::DestroyEntity { .. });

        let entity_id = EntityId::new(42);

        let mut ws1 = WriteSet::new();
        ws1.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(100.0),
        });
        ws1.push(PendingWrite::DestroyEntity { id: entity_id });

        let mut ws2 = WriteSet::new();
        ws2.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(200.0),
        });
        ws2.push(PendingWrite::DestroyEntity { id: entity_id });

        // With only_destroys filter: just the destroy conflict
        let report =
            detect_conflicts_filtered(&[(CoreId(0), ws1), (CoreId(1), ws2)], only_destroys);
        assert_eq!(report.len(), 1);
        assert!(matches!(
            &report.conflicts[0].kind,
            ConflictKind::DestroyEntityWriteWrite { .. }
        ));
    }

    // ========================================================================
    // Tests for deterministic ordering
    // ========================================================================

    #[test]
    fn test_deterministic_core_ordering() {
        // Cores should always be sorted in the output regardless of input order
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
            key: "gold".to_string(),
            value: Value::Float(300.0),
        });

        // Test with different input orders - output should be the same
        let report1 = detect_conflicts(&[
            (CoreId(2), ws2.clone()),
            (CoreId(0), ws0.clone()),
            (CoreId(1), ws1.clone()),
        ]);
        let report2 = detect_conflicts(&[(CoreId(0), ws0), (CoreId(1), ws1), (CoreId(2), ws2)]);

        // Both should have cores sorted: [0, 1, 2]
        assert_eq!(
            report1.conflicts[0].cores,
            vec![CoreId(0), CoreId(1), CoreId(2)]
        );
        assert_eq!(
            report2.conflicts[0].cores,
            vec![CoreId(0), CoreId(1), CoreId(2)]
        );

        // ConflictKind should have core_a=0, core_b=1 (first two sorted)
        match &report1.conflicts[0].kind {
            ConflictKind::GlobalPropertyWriteWrite { core_a, core_b, .. } => {
                assert_eq!(*core_a, CoreId(0));
                assert_eq!(*core_b, CoreId(1));
            }
            _ => panic!("Expected GlobalPropertyWriteWrite"),
        }
    }

    #[test]
    fn test_conflict_cores_field() {
        // Verify the cores field contains all distinct cores
        let mut ws0 = WriteSet::new();
        ws0.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(100.0),
        });
        // Core 0 writes twice
        ws0.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(150.0),
        });

        let mut ws1 = WriteSet::new();
        ws1.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(200.0),
        });

        let mut ws2 = WriteSet::new();
        ws2.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(300.0),
        });

        let report = detect_conflicts(&[(CoreId(0), ws0), (CoreId(1), ws1), (CoreId(2), ws2)]);

        // Should have exactly 3 distinct cores (even though Core 0 wrote twice)
        assert_eq!(report.conflicts[0].cores.len(), 3);
        assert_eq!(
            report.conflicts[0].cores,
            vec![CoreId(0), CoreId(1), CoreId(2)]
        );
        assert_eq!(report.conflicts[0].core_count(), 3);

        // Should have 4 total writes
        assert_eq!(report.conflicts[0].writes.len(), 4);
    }
}
