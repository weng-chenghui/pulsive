//! Conflict detection for parallel WriteSet merging
//!
//! When multiple cores execute in parallel, they may produce conflicting writes.
//! This module detects such conflicts before merging WriteSets.
//!
//! # Architecture
//!
//! The conflict system uses a clean separation of concerns:
//!
//! - [`ConflictTarget`]: What was conflicted on (entity property, global, etc.)
//! - [`ConflictType`]: Type of conflict (write-write, read-write in future)
//! - [`Conflict`]: Full conflict record with target, type, cores, and writes
//!
//! # Conflict Types
//!
//! - **Write-Write**: Two different cores wrote to the same (entity, property) or global
//! - **Read-Write** (future): Core A read what Core B wrote
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

// Re-export WriteSet for convenience in resolution result
pub use pulsive_core::WriteSet as WriteSetCore;

/// The target of a conflict - what resource was conflicted on
///
/// This enum identifies the specific resource (entity property, global, etc.)
/// that multiple cores attempted to write to. It serves as both the key for
/// conflict detection and part of the conflict report.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConflictTarget {
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

impl ConflictTarget {
    /// Extract the target from a PendingWrite
    pub fn from_pending_write(write: &PendingWrite) -> Self {
        match write {
            PendingWrite::SetProperty { entity_id, key, .. } => ConflictTarget::EntityProperty {
                entity_id: *entity_id,
                property: key.clone(),
            },
            PendingWrite::ModifyProperty { entity_id, key, .. } => ConflictTarget::EntityProperty {
                entity_id: *entity_id,
                property: key.clone(),
            },
            PendingWrite::SetGlobal { key, .. } => ConflictTarget::GlobalProperty {
                property: key.clone(),
            },
            PendingWrite::ModifyGlobal { key, .. } => ConflictTarget::GlobalProperty {
                property: key.clone(),
            },
            PendingWrite::AddFlag { entity_id, flag } => ConflictTarget::EntityFlag {
                entity_id: *entity_id,
                flag: flag.clone(),
            },
            PendingWrite::RemoveFlag { entity_id, flag } => ConflictTarget::EntityFlag {
                entity_id: *entity_id,
                flag: flag.clone(),
            },
            PendingWrite::SpawnEntity { kind, .. } => {
                ConflictTarget::SpawnEntity { kind: kind.clone() }
            }
            PendingWrite::DestroyEntity { id } => ConflictTarget::DestroyEntity { entity_id: *id },
        }
    }
}

impl std::fmt::Display for ConflictTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConflictTarget::EntityProperty {
                entity_id,
                property,
            } => write!(f, "entity {} property '{}'", entity_id, property),
            ConflictTarget::EntityFlag { entity_id, flag } => {
                write!(f, "entity {} flag '{}'", entity_id, flag)
            }
            ConflictTarget::GlobalProperty { property } => write!(f, "global '{}'", property),
            ConflictTarget::SpawnEntity { kind } => write!(f, "spawn entity kind '{}'", kind),
            ConflictTarget::DestroyEntity { entity_id } => {
                write!(f, "destroy entity {}", entity_id)
            }
        }
    }
}

/// The type of conflict detected
///
/// This enum classifies the nature of the conflict. Currently only write-write
/// conflicts are detected, but the design allows for future read-write conflict
/// detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ConflictType {
    /// Two or more cores wrote to the same target
    #[default]
    WriteWrite,
    /// A core read a value that another core wrote (future)
    ///
    /// This variant is reserved for future read-write conflict detection.
    /// When implemented, it will detect cases where Core A's read depends on
    /// Core B's write, which can cause non-deterministic behavior.
    #[doc(hidden)]
    ReadWrite,
}

impl std::fmt::Display for ConflictType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConflictType::WriteWrite => write!(f, "write-write"),
            ConflictType::ReadWrite => write!(f, "read-write"),
        }
    }
}

/// A detected conflict with diagnostic information
///
/// Contains all information needed for conflict resolution:
/// - `target`: The resource that was conflicted on
/// - `conflict_type`: The type of conflict (write-write, read-write)
/// - `cores`: All distinct cores involved (sorted by ID for deterministic output)
/// - `writes`: All conflicting writes for debugging/resolution
///
/// # Example
///
/// ```ignore
/// match &conflict.target {
///     ConflictTarget::GlobalProperty { property } => {
///         println!("{} conflict on {} between {:?}",
///             conflict.conflict_type, property, conflict.cores);
///     }
///     // ...
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Conflict {
    /// The target of the conflict (what resource was conflicted on)
    pub target: ConflictTarget,

    /// The type of conflict (write-write, read-write)
    pub conflict_type: ConflictType,

    /// All distinct cores involved in this conflict (sorted by CoreId for determinism)
    pub cores: Vec<CoreId>,

    /// All conflicting writes from all cores (for debugging/resolution)
    pub writes: Vec<(CoreId, PendingWrite)>,

    /// Reserved for future read-write conflict detection
    ///
    /// When read-write conflict detection is implemented, this field will contain
    /// the reads that conflict with writes from other cores.
    #[doc(hidden)]
    pub reads: Vec<(CoreId, ReadRecord)>,
}

/// Record of a read operation (for future read-write conflict detection)
#[derive(Debug, Clone, PartialEq)]
#[doc(hidden)]
pub struct ReadRecord {
    /// The target that was read
    pub target: ConflictTarget,
}

impl Conflict {
    /// Create a new conflict
    ///
    /// # Panics (debug mode)
    ///
    /// Panics if `cores` has fewer than 2 elements, as a conflict requires
    /// at least two distinct cores.
    pub fn new(
        target: ConflictTarget,
        conflict_type: ConflictType,
        cores: Vec<CoreId>,
        writes: Vec<(CoreId, PendingWrite)>,
    ) -> Self {
        debug_assert!(
            cores.len() >= 2,
            "Conflict::new requires at least 2 cores, got {}",
            cores.len()
        );

        Self {
            target,
            conflict_type,
            cores,
            writes,
            reads: Vec::new(),
        }
    }

    /// Get the number of cores involved in this conflict
    pub fn core_count(&self) -> usize {
        self.cores.len()
    }

    /// Check if this is a write-write conflict
    pub fn is_write_write(&self) -> bool {
        self.conflict_type == ConflictType::WriteWrite
    }

    /// Check if this is a read-write conflict
    pub fn is_read_write(&self) -> bool {
        self.conflict_type == ConflictType::ReadWrite
    }
}

impl std::fmt::Display for Conflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Format cores as a comma-separated list: "Core(0), Core(1)"
        let cores_str: Vec<String> = self.cores.iter().map(|c| format!("{}", c)).collect();
        write!(
            f,
            "{} conflict on {} between {}",
            self.conflict_type,
            self.target,
            cores_str.join(", ")
        )
    }
}

/// Result of conflict detection
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConflictReport {
    /// All detected conflicts
    pub conflicts: Vec<Conflict>,
}

impl std::fmt::Display for ConflictReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.conflicts.len();
        if count == 1 {
            write!(f, "1 conflict")
        } else {
            write!(f, "{} conflicts", count)
        }
    }
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
/// 1. Build a HashMap from `ConflictTarget` to `Vec<(CoreId, PendingWrite)>`
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
    let mut write_map: HashMap<ConflictTarget, Vec<(CoreId, PendingWrite)>> = HashMap::new();

    // Phase 1: Collect all writes by target
    for (core_id, ws) in write_sets {
        for write in ws.iter() {
            let target = ConflictTarget::from_pending_write(write);
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
            let conflict = create_conflict(target, writes, distinct_cores);
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
    target: ConflictTarget,
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

    // Create conflict using the new API
    Conflict::new(target, ConflictType::WriteWrite, sorted_cores, writes)
}

/// Detect conflicts with an option to exclude certain target types
///
/// This is useful for cases where spawn conflicts are acceptable
/// (e.g., multiple cores spawning entities of the same kind is often fine).
/// Use `default_conflict_filter` to exclude spawn conflicts.
pub fn detect_conflicts_filtered<F>(write_sets: &[(CoreId, WriteSet)], filter: F) -> ConflictReport
where
    F: Fn(&ConflictTarget) -> bool,
{
    let mut write_map: HashMap<ConflictTarget, Vec<(CoreId, PendingWrite)>> = HashMap::new();

    for (core_id, ws) in write_sets {
        for write in ws.iter() {
            let target = ConflictTarget::from_pending_write(write);
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
            let conflict = create_conflict(target, writes, distinct_cores);
            report.conflicts.push(conflict);
        }
    }

    report
}

/// Default filter that excludes spawn conflicts (which are usually acceptable)
pub fn default_conflict_filter(target: &ConflictTarget) -> bool {
    !matches!(target, ConflictTarget::SpawnEntity { .. })
}

// ============================================================================
// Conflict Resolution
// ============================================================================

/// A custom conflict resolver function type
///
/// Takes a reference to a `Conflict` and returns the winning write (if any).
/// Return `None` to skip the conflicting write entirely.
pub type ConflictResolver = Box<dyn Fn(&Conflict) -> Option<(CoreId, PendingWrite)> + Send + Sync>;

/// Strategy for resolving write-write conflicts
///
/// When multiple cores write to the same target, the hub needs to decide
/// how to handle the conflict. This enum defines the available strategies.
///
/// # Example
///
/// ```
/// use pulsive_hub::{ResolutionStrategy, CoreId};
///
/// // Use first-write-wins for deterministic resolution
/// let strategy = ResolutionStrategy::FirstWriteWins;
///
/// // Or use a custom resolver for complex logic
/// let strategy = ResolutionStrategy::Custom(Box::new(|conflict| {
///     // Always pick the write from the core with the lowest ID
///     conflict.writes.first().cloned()
/// }));
/// ```
#[derive(Default)]
pub enum ResolutionStrategy {
    /// Abort on any conflict - return an error
    ///
    /// This is the safest strategy: if any conflict is detected, the entire
    /// merge operation fails and returns an error with the conflict report.
    /// Use this when conflicts indicate a logic error that needs fixing.
    #[default]
    Abort,

    /// Last write wins - take the write from the highest-CoreId core
    ///
    /// For deterministic resolution, "last" is defined as the core with the
    /// highest CoreId value. This ensures consistent results across runs.
    LastWriteWins,

    /// First write wins - take the write from the lowest-CoreId core
    ///
    /// For deterministic resolution, "first" is defined as the core with the
    /// lowest CoreId value. This ensures consistent results across runs.
    FirstWriteWins,

    /// Merge numeric operations when possible
    ///
    /// For numeric modifications (Add, Sub), this strategy combines the values.
    /// For non-mergeable operations (Set, Mul, Div), falls back to FirstWriteWins.
    ///
    /// Examples:
    /// - Core A: Add 10, Core B: Add 20 → Add 30
    /// - Core A: Sub 5, Core B: Sub 10 → Sub 15
    /// - Core A: Set 100, Core B: Set 200 → Set 100 (first wins)
    Merge,

    /// Custom resolution function
    ///
    /// Provides full control over conflict resolution. The function receives
    /// the conflict details and returns the write to use, or None to skip.
    Custom(ConflictResolver),
}

impl std::fmt::Debug for ResolutionStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolutionStrategy::Abort => write!(f, "Abort"),
            ResolutionStrategy::LastWriteWins => write!(f, "LastWriteWins"),
            ResolutionStrategy::FirstWriteWins => write!(f, "FirstWriteWins"),
            ResolutionStrategy::Merge => write!(f, "Merge"),
            ResolutionStrategy::Custom(_) => write!(f, "Custom(<fn>)"),
        }
    }
}

/// Result of conflict resolution
///
/// Contains the merged WriteSet with all conflicts resolved, plus an audit trail
/// of how each conflict was resolved for debugging and logging purposes.
#[derive(Debug, Clone)]
pub struct ResolutionResult {
    /// The resolved WriteSet (conflicts have been resolved)
    ///
    /// Contains all non-conflicting writes from all cores, plus the winning
    /// write for each conflict (as determined by the resolution strategy).
    pub write_set: WriteSet,

    /// Number of conflicts that were resolved
    pub conflicts_resolved: usize,

    /// Details of each resolution (for auditing/debugging)
    ///
    /// Each entry corresponds to one conflict that was resolved. The order
    /// matches the order in which conflicts were processed (which may vary
    /// between runs due to HashMap iteration order).
    pub resolutions: Vec<ResolvedConflict>,
}

/// Details about how a conflict was resolved
///
/// Provides an audit trail for each resolved conflict, useful for debugging
/// and logging. This allows consumers to understand which writes were in
/// conflict and which one was selected.
#[derive(Debug, Clone)]
pub struct ResolvedConflict {
    /// The target that had a conflict (e.g., which entity/property)
    pub target: ConflictTarget,

    /// The type of conflict that was resolved
    pub conflict_type: ConflictType,

    /// The cores involved in the conflict (sorted by CoreId for determinism)
    ///
    /// This matches the `cores` field from the original `Conflict` that was
    /// resolved. The order is deterministic (sorted by CoreId).
    pub cores: Vec<CoreId>,

    /// The winning write selected by the resolution strategy
    ///
    /// - `Some((core_id, write))`: The write that was selected, along with
    ///   which core it came from. For merge strategies, `core_id` is the
    ///   lowest CoreId involved (used as a representative owner).
    /// - `None`: The conflict was resolved by skipping/dropping the write
    ///   (only possible with custom resolvers that return `None`).
    pub resolved_write: Option<(CoreId, PendingWrite)>,
}

impl ResolutionResult {
    /// Create a new resolution result
    pub fn new(write_set: WriteSet) -> Self {
        Self {
            write_set,
            conflicts_resolved: 0,
            resolutions: Vec::new(),
        }
    }

    /// Add a resolution record
    pub fn add_resolution(&mut self, resolution: ResolvedConflict) {
        self.conflicts_resolved += 1;
        self.resolutions.push(resolution);
    }
}

/// Resolve conflicts in WriteSets using the specified strategy
///
/// This function detects conflicts across the provided WriteSets and resolves
/// them according to the given strategy. If the strategy is `Abort` and
/// conflicts are detected, returns an error.
///
/// # Arguments
///
/// * `write_sets` - WriteSets from each core with their CoreIds
/// * `strategy` - How to resolve detected conflicts
///
/// # Returns
///
/// * `Ok(ResolutionResult)` - Successfully merged WriteSet with resolution details
/// * `Err(Error::UnresolvedConflicts)` - If strategy is `Abort` and conflicts exist
///
/// # Example
///
/// ```
/// use pulsive_hub::{resolve_conflicts, ResolutionStrategy, CoreId};
/// use pulsive_core::{WriteSet, PendingWrite, Value};
///
/// let mut ws0 = WriteSet::new();
/// ws0.push(PendingWrite::SetGlobal {
///     key: "gold".to_string(),
///     value: Value::Float(100.0),
/// });
///
/// let mut ws1 = WriteSet::new();
/// ws1.push(PendingWrite::SetGlobal {
///     key: "gold".to_string(),
///     value: Value::Float(200.0),
/// });
///
/// let write_sets = vec![(CoreId(0), ws0), (CoreId(1), ws1)];
///
/// // With FirstWriteWins, Core 0's value (100) will be used
/// let result = resolve_conflicts(&write_sets, &ResolutionStrategy::FirstWriteWins).unwrap();
/// assert_eq!(result.conflicts_resolved, 1);
/// ```
pub fn resolve_conflicts(
    write_sets: &[(CoreId, WriteSet)],
    strategy: &ResolutionStrategy,
) -> crate::Result<ResolutionResult> {
    // Detect all conflicts first
    let report = detect_conflicts(write_sets);

    // If no conflicts, just merge the WriteSets
    if !report.has_conflicts() {
        let merged = WriteSet::merge(write_sets.iter().map(|(_, ws)| ws.clone()).collect());
        return Ok(ResolutionResult::new(merged));
    }

    // Handle based on strategy
    match strategy {
        ResolutionStrategy::Abort => Err(crate::Error::unresolved_conflicts(report)),

        ResolutionStrategy::FirstWriteWins => {
            resolve_with_strategy(write_sets, &report, |conflict| {
                // First write = lowest CoreId
                conflict
                    .writes
                    .iter()
                    .min_by_key(|(core_id, _)| core_id.0)
                    .cloned()
            })
        }

        ResolutionStrategy::LastWriteWins => {
            resolve_with_strategy(write_sets, &report, |conflict| {
                // Last write = highest CoreId
                conflict
                    .writes
                    .iter()
                    .max_by_key(|(core_id, _)| core_id.0)
                    .cloned()
            })
        }

        ResolutionStrategy::Merge => resolve_with_merge(write_sets, &report),

        ResolutionStrategy::Custom(resolver) => {
            resolve_with_strategy(write_sets, &report, resolver)
        }
    }
}

/// Helper function to resolve conflicts using a picker function
fn resolve_with_strategy<F>(
    write_sets: &[(CoreId, WriteSet)],
    report: &ConflictReport,
    picker: F,
) -> crate::Result<ResolutionResult>
where
    F: Fn(&Conflict) -> Option<(CoreId, PendingWrite)>,
{
    // Build a set of conflicting targets for quick lookup
    let conflicting_targets: HashSet<ConflictTarget> =
        report.conflicts.iter().map(|c| c.target.clone()).collect();

    let mut result = ResolutionResult::new(WriteSet::new());

    // First, add all non-conflicting writes
    for (_, ws) in write_sets {
        for write in ws.iter() {
            let target = ConflictTarget::from_pending_write(write);
            if !conflicting_targets.contains(&target) {
                result.write_set.push(write.clone());
            }
        }
    }

    // Then, resolve each conflict
    for conflict in &report.conflicts {
        let resolved_write = picker(conflict);

        if let Some((_, write)) = &resolved_write {
            result.write_set.push(write.clone());
        }

        result.add_resolution(ResolvedConflict {
            target: conflict.target.clone(),
            conflict_type: conflict.conflict_type,
            cores: conflict.cores.clone(),
            resolved_write,
        });
    }

    Ok(result)
}

/// Helper function for merge strategy - combines compatible operations
///
/// # Merge Behavior
///
/// - **Add operations**: All values are summed (e.g., Add(10) + Add(20) = Add(30))
/// - **Sub operations**: All values are summed (e.g., Sub(5) + Sub(10) = Sub(15))
/// - **Non-mergeable operations**: Falls back to FirstWriteWins
///
/// # Core Attribution
///
/// For merged writes, the resulting write is attributed to the lowest CoreId
/// involved in the conflict (`conflict.cores[0]`). This is a representative
/// owner for audit purposes - the actual write is a combination of all cores'
/// contributions.
fn resolve_with_merge(
    write_sets: &[(CoreId, WriteSet)],
    report: &ConflictReport,
) -> crate::Result<ResolutionResult> {
    use pulsive_core::ModifyOp;

    resolve_with_strategy(write_sets, report, |conflict| {
        // Check if all writes are mergeable numeric operations
        let all_add = conflict.writes.iter().all(|(_, w)| {
            matches!(
                w,
                PendingWrite::ModifyProperty {
                    op: ModifyOp::Add,
                    ..
                } | PendingWrite::ModifyGlobal {
                    op: ModifyOp::Add,
                    ..
                }
            )
        });

        let all_sub = conflict.writes.iter().all(|(_, w)| {
            matches!(
                w,
                PendingWrite::ModifyProperty {
                    op: ModifyOp::Sub,
                    ..
                } | PendingWrite::ModifyGlobal {
                    op: ModifyOp::Sub,
                    ..
                }
            )
        });

        if all_add {
            // Sum all Add values; attribute merged write to lowest CoreId
            let merged = merge_modify_writes(&conflict.writes, ModifyOp::Add);
            merged.map(|w| (conflict.cores[0], w))
        } else if all_sub {
            // Sum all Sub values; attribute merged write to lowest CoreId
            let merged = merge_modify_writes(&conflict.writes, ModifyOp::Sub);
            merged.map(|w| (conflict.cores[0], w))
        } else {
            // Fall back to first-write-wins for non-mergeable operations
            conflict
                .writes
                .iter()
                .min_by_key(|(core_id, _)| core_id.0)
                .cloned()
        }
    })
}

/// Merge multiple modify operations into one
fn merge_modify_writes(
    writes: &[(CoreId, PendingWrite)],
    op: pulsive_core::ModifyOp,
) -> Option<PendingWrite> {
    let first = writes.first()?;

    match &first.1 {
        PendingWrite::ModifyProperty { entity_id, key, .. } => {
            let total: f64 = writes
                .iter()
                .filter_map(|(_, w)| match w {
                    PendingWrite::ModifyProperty { value, .. } => Some(*value),
                    _ => None,
                })
                .sum();

            Some(PendingWrite::ModifyProperty {
                entity_id: *entity_id,
                key: key.clone(),
                op,
                value: total,
            })
        }
        PendingWrite::ModifyGlobal { key, .. } => {
            let total: f64 = writes
                .iter()
                .filter_map(|(_, w)| match w {
                    PendingWrite::ModifyGlobal { value, .. } => Some(*value),
                    _ => None,
                })
                .sum();

            Some(PendingWrite::ModifyGlobal {
                key: key.clone(),
                op,
                value: total,
            })
        }
        _ => None,
    }
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

        let conflict = &report.conflicts[0];
        match &conflict.target {
            ConflictTarget::GlobalProperty { property } => {
                assert_eq!(property, "gold");
            }
            _ => panic!("Expected GlobalProperty target"),
        }
        assert_eq!(conflict.conflict_type, ConflictType::WriteWrite);
        assert_eq!(conflict.cores, vec![CoreId(0), CoreId(1)]);
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

        match &report.conflicts[0].target {
            ConflictTarget::EntityProperty {
                entity_id: eid,
                property,
            } => {
                assert_eq!(*eid, entity_id);
                assert_eq!(property, "health");
            }
            _ => panic!("Expected EntityProperty target"),
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

        match &report.conflicts[0].target {
            ConflictTarget::EntityFlag {
                entity_id: eid,
                flag: f,
            } => {
                assert_eq!(*eid, entity_id);
                assert_eq!(*f, flag);
            }
            _ => panic!("Expected EntityFlag target"),
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

        match &report.conflicts[0].target {
            ConflictTarget::DestroyEntity { entity_id: eid } => {
                assert_eq!(*eid, entity_id);
            }
            _ => panic!("Expected DestroyEntity target"),
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
        // All 3 cores should be listed
        assert_eq!(
            report.conflicts[0].cores,
            vec![CoreId(0), CoreId(1), CoreId(2)]
        );
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

        // Verify it's a SpawnEntity conflict
        match &report.conflicts[0].target {
            ConflictTarget::SpawnEntity { kind: k } => {
                assert_eq!(k.as_str(), "unit");
            }
            _ => panic!("Expected SpawnEntity target"),
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
            ConflictTarget::GlobalProperty {
                property: "gold".to_string(),
            },
            ConflictType::WriteWrite,
            vec![CoreId(0), CoreId(1)],
            vec![],
        );

        let display = format!("{}", conflict);
        assert!(display.contains("gold"));
        assert!(display.contains("write-write"));
        // Should now include actual core IDs
        assert!(display.contains("Core(0)"));
        assert!(display.contains("Core(1)"));
    }

    #[test]
    fn test_conflict_target_extraction() {
        let entity_id = EntityId::new(42);

        // SetProperty
        let write = PendingWrite::SetProperty {
            entity_id,
            key: "health".to_string(),
            value: Value::Float(100.0),
        };
        let target = ConflictTarget::from_pending_write(&write);
        assert!(matches!(
            target,
            ConflictTarget::EntityProperty {
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
        let target = ConflictTarget::from_pending_write(&write);
        assert!(matches!(
            target,
            ConflictTarget::GlobalProperty { property } if property == "gold"
        ));

        // DestroyEntity
        let write = PendingWrite::DestroyEntity { id: entity_id };
        let target = ConflictTarget::from_pending_write(&write);
        assert!(matches!(
            target,
            ConflictTarget::DestroyEntity { entity_id: eid } if eid == entity_id
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

        // Use new API to verify conflict
        match &report.conflicts[0].target {
            ConflictTarget::GlobalProperty { property } => {
                assert_eq!(property, "gold");
            }
            _ => panic!("Expected GlobalProperty target"),
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
            ConflictTarget::SpawnEntity {
                kind: DefId::new("unit"),
            },
            ConflictType::WriteWrite,
            vec![CoreId(0), CoreId(1)],
            vec![],
        );

        let display = format!("{}", conflict);
        assert!(display.contains("unit"));
        assert!(display.contains("spawn"));
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
            |target: &ConflictTarget| !matches!(target, ConflictTarget::GlobalProperty { .. });

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
            &report_filtered.conflicts[0].target,
            ConflictTarget::EntityProperty { .. }
        ));
    }

    #[test]
    fn test_custom_filter_only_destroys() {
        // Custom filter that only includes destroy conflicts
        let only_destroys =
            |target: &ConflictTarget| matches!(target, ConflictTarget::DestroyEntity { .. });

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
            &report.conflicts[0].target,
            ConflictTarget::DestroyEntity { .. }
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

        // Verify target is correct
        assert!(matches!(
            &report1.conflicts[0].target,
            ConflictTarget::GlobalProperty { property } if property == "gold"
        ));
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

    // ========================================================================
    // Conflict Resolution Tests
    // ========================================================================

    #[test]
    fn test_resolve_no_conflicts() {
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

        let write_sets = vec![(CoreId(0), ws0), (CoreId(1), ws1)];

        let result = resolve_conflicts(&write_sets, &ResolutionStrategy::Abort).unwrap();
        assert_eq!(result.conflicts_resolved, 0);
        assert_eq!(result.write_set.len(), 2);
    }

    #[test]
    fn test_resolve_abort_on_conflict() {
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

        let write_sets = vec![(CoreId(0), ws0), (CoreId(1), ws1)];

        let result = resolve_conflicts(&write_sets, &ResolutionStrategy::Abort);
        assert!(result.is_err());

        match result {
            Err(crate::Error::UnresolvedConflicts { count, report }) => {
                assert_eq!(count, 1);
                assert_eq!(report.len(), 1);
            }
            _ => panic!("Expected UnresolvedConflicts error"),
        }
    }

    #[test]
    fn test_resolve_first_write_wins() {
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

        let write_sets = vec![(CoreId(0), ws0), (CoreId(1), ws1)];

        let result = resolve_conflicts(&write_sets, &ResolutionStrategy::FirstWriteWins).unwrap();
        assert_eq!(result.conflicts_resolved, 1);
        assert_eq!(result.write_set.len(), 1);

        // First write wins means Core 0's value (100) should win
        match result.write_set.iter().next() {
            Some(PendingWrite::SetGlobal { key, value }) => {
                assert_eq!(key, "gold");
                assert_eq!(value.as_float(), Some(100.0));
            }
            _ => panic!("Expected SetGlobal write"),
        }

        // Check resolution details
        assert_eq!(result.resolutions.len(), 1);
        let resolution = &result.resolutions[0];
        assert_eq!(resolution.cores, vec![CoreId(0), CoreId(1)]);
        assert!(resolution.resolved_write.is_some());
    }

    #[test]
    fn test_resolve_last_write_wins() {
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

        let write_sets = vec![(CoreId(0), ws0), (CoreId(1), ws1)];

        let result = resolve_conflicts(&write_sets, &ResolutionStrategy::LastWriteWins).unwrap();
        assert_eq!(result.conflicts_resolved, 1);
        assert_eq!(result.write_set.len(), 1);

        // Last write wins means Core 1's value (200) should win
        let writes: Vec<_> = result.write_set.iter().collect();
        match &writes[0] {
            PendingWrite::SetGlobal { key, value } => {
                assert_eq!(key, "gold");
                assert_eq!(value.as_float(), Some(200.0));
            }
            _ => panic!("Expected SetGlobal write"),
        }
    }

    #[test]
    fn test_resolve_merge_add_operations() {
        let mut ws0 = WriteSet::new();
        ws0.push(PendingWrite::ModifyGlobal {
            key: "gold".to_string(),
            op: ModifyOp::Add,
            value: 10.0,
        });

        let mut ws1 = WriteSet::new();
        ws1.push(PendingWrite::ModifyGlobal {
            key: "gold".to_string(),
            op: ModifyOp::Add,
            value: 20.0,
        });

        let mut ws2 = WriteSet::new();
        ws2.push(PendingWrite::ModifyGlobal {
            key: "gold".to_string(),
            op: ModifyOp::Add,
            value: 30.0,
        });

        let write_sets = vec![(CoreId(0), ws0), (CoreId(1), ws1), (CoreId(2), ws2)];

        let result = resolve_conflicts(&write_sets, &ResolutionStrategy::Merge).unwrap();
        assert_eq!(result.conflicts_resolved, 1);
        assert_eq!(result.write_set.len(), 1);

        // Merge should sum all Add values: 10 + 20 + 30 = 60
        let writes: Vec<_> = result.write_set.iter().collect();
        match &writes[0] {
            PendingWrite::ModifyGlobal { key, op, value } => {
                assert_eq!(key, "gold");
                assert!(matches!(op, ModifyOp::Add));
                assert!((value - 60.0).abs() < f64::EPSILON);
            }
            _ => panic!("Expected ModifyGlobal write"),
        }
    }

    #[test]
    fn test_resolve_merge_sub_operations() {
        let entity_id = EntityId::new(42);

        let mut ws0 = WriteSet::new();
        ws0.push(PendingWrite::ModifyProperty {
            entity_id,
            key: "health".to_string(),
            op: ModifyOp::Sub,
            value: 10.0,
        });

        let mut ws1 = WriteSet::new();
        ws1.push(PendingWrite::ModifyProperty {
            entity_id,
            key: "health".to_string(),
            op: ModifyOp::Sub,
            value: 25.0,
        });

        let write_sets = vec![(CoreId(0), ws0), (CoreId(1), ws1)];

        let result = resolve_conflicts(&write_sets, &ResolutionStrategy::Merge).unwrap();
        assert_eq!(result.conflicts_resolved, 1);
        assert_eq!(result.write_set.len(), 1);

        // Merge should sum all Sub values: 10 + 25 = 35
        let writes: Vec<_> = result.write_set.iter().collect();
        match &writes[0] {
            PendingWrite::ModifyProperty {
                entity_id: eid,
                key,
                op,
                value,
            } => {
                assert_eq!(*eid, entity_id);
                assert_eq!(key, "health");
                assert!(matches!(op, ModifyOp::Sub));
                assert!((value - 35.0).abs() < f64::EPSILON);
            }
            _ => panic!("Expected ModifyProperty write"),
        }
    }

    #[test]
    fn test_resolve_merge_non_mergeable_falls_back() {
        // Set operations cannot be merged, should fall back to first-write-wins
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

        let write_sets = vec![(CoreId(0), ws0), (CoreId(1), ws1)];

        let result = resolve_conflicts(&write_sets, &ResolutionStrategy::Merge).unwrap();
        assert_eq!(result.conflicts_resolved, 1);

        // Should fall back to first write (Core 0's value)
        let writes: Vec<_> = result.write_set.iter().collect();
        match &writes[0] {
            PendingWrite::SetGlobal { key, value } => {
                assert_eq!(key, "gold");
                assert_eq!(value.as_float(), Some(100.0));
            }
            _ => panic!("Expected SetGlobal write"),
        }
    }

    #[test]
    fn test_resolve_custom_strategy() {
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

        let write_sets = vec![(CoreId(0), ws0), (CoreId(1), ws1)];

        // Custom strategy: always pick Core 1's value (last)
        let strategy = ResolutionStrategy::Custom(Box::new(|conflict| {
            conflict.writes.iter().find(|(c, _)| c.0 == 1).cloned()
        }));

        let result = resolve_conflicts(&write_sets, &strategy).unwrap();
        assert_eq!(result.conflicts_resolved, 1);

        let writes: Vec<_> = result.write_set.iter().collect();
        match &writes[0] {
            PendingWrite::SetGlobal { key, value } => {
                assert_eq!(key, "gold");
                assert_eq!(value.as_float(), Some(200.0));
            }
            _ => panic!("Expected SetGlobal write"),
        }
    }

    #[test]
    fn test_resolve_custom_strategy_skip_write() {
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

        let write_sets = vec![(CoreId(0), ws0), (CoreId(1), ws1)];

        // Custom strategy that returns None (skip the write)
        let strategy = ResolutionStrategy::Custom(Box::new(|_| None));

        let result = resolve_conflicts(&write_sets, &strategy).unwrap();
        assert_eq!(result.conflicts_resolved, 1);
        // No writes should be in the result (the conflicting write was skipped)
        assert_eq!(result.write_set.len(), 0);

        // Resolution should record that the write was skipped
        assert!(result.resolutions[0].resolved_write.is_none());
    }

    #[test]
    fn test_resolve_mixed_conflicting_and_non_conflicting() {
        let entity_id = EntityId::new(42);

        let mut ws0 = WriteSet::new();
        // Conflicting write
        ws0.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(100.0),
        });
        // Non-conflicting write
        ws0.push(PendingWrite::SetProperty {
            entity_id,
            key: "health".to_string(),
            value: Value::Float(100.0),
        });

        let mut ws1 = WriteSet::new();
        // Conflicting write
        ws1.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(200.0),
        });
        // Non-conflicting write (different property)
        ws1.push(PendingWrite::SetProperty {
            entity_id,
            key: "mana".to_string(),
            value: Value::Float(50.0),
        });

        let write_sets = vec![(CoreId(0), ws0), (CoreId(1), ws1)];

        let result = resolve_conflicts(&write_sets, &ResolutionStrategy::FirstWriteWins).unwrap();
        assert_eq!(result.conflicts_resolved, 1);
        // Should have: resolved gold + health + mana = 3 writes
        assert_eq!(result.write_set.len(), 3);
    }

    #[test]
    fn test_resolve_multiple_conflicts() {
        let entity_id = EntityId::new(42);

        let mut ws0 = WriteSet::new();
        ws0.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(100.0),
        });
        ws0.push(PendingWrite::SetProperty {
            entity_id,
            key: "health".to_string(),
            value: Value::Float(100.0),
        });

        let mut ws1 = WriteSet::new();
        ws1.push(PendingWrite::SetGlobal {
            key: "gold".to_string(),
            value: Value::Float(200.0),
        });
        ws1.push(PendingWrite::SetProperty {
            entity_id,
            key: "health".to_string(),
            value: Value::Float(50.0),
        });

        let write_sets = vec![(CoreId(0), ws0), (CoreId(1), ws1)];

        let result = resolve_conflicts(&write_sets, &ResolutionStrategy::LastWriteWins).unwrap();
        assert_eq!(result.conflicts_resolved, 2);
        assert_eq!(result.write_set.len(), 2);

        // Both should be Core 1's values (last write wins)
        for write in result.write_set.iter() {
            match write {
                PendingWrite::SetGlobal { key, value } => {
                    assert_eq!(key, "gold");
                    assert_eq!(value.as_float(), Some(200.0));
                }
                PendingWrite::SetProperty { key, value, .. } => {
                    assert_eq!(key, "health");
                    assert_eq!(value.as_float(), Some(50.0));
                }
                _ => panic!("Unexpected write type"),
            }
        }
    }

    #[test]
    fn test_resolution_strategy_debug() {
        assert_eq!(format!("{:?}", ResolutionStrategy::Abort), "Abort");
        assert_eq!(
            format!("{:?}", ResolutionStrategy::FirstWriteWins),
            "FirstWriteWins"
        );
        assert_eq!(
            format!("{:?}", ResolutionStrategy::LastWriteWins),
            "LastWriteWins"
        );
        assert_eq!(format!("{:?}", ResolutionStrategy::Merge), "Merge");
        assert_eq!(
            format!("{:?}", ResolutionStrategy::Custom(Box::new(|_| None))),
            "Custom(<fn>)"
        );
    }

    #[test]
    fn test_resolution_strategy_default() {
        let strategy = ResolutionStrategy::default();
        assert!(matches!(strategy, ResolutionStrategy::Abort));
    }

    // ========================================================================
    // Thread Safety and Error Handling Tests
    // ========================================================================

    /// Compile-time check that ConflictReport is Send + Sync
    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn test_conflict_report_is_send_sync() {
        // This test verifies ConflictReport can be safely shared across threads
        _assert_send_sync::<ConflictReport>();
        _assert_send_sync::<Conflict>();
        _assert_send_sync::<ConflictTarget>();
        _assert_send_sync::<ConflictType>();
        _assert_send_sync::<ResolutionResult>();
        _assert_send_sync::<ResolvedConflict>();
    }

    #[test]
    fn test_error_display_roundtrip() {
        // Test that the error message formats correctly
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

        let write_sets = vec![(CoreId(0), ws0), (CoreId(1), ws1)];

        let result = resolve_conflicts(&write_sets, &ResolutionStrategy::Abort);
        let err = result.unwrap_err();

        // Verify the error message contains the count
        let error_message = err.to_string();
        assert!(
            error_message.contains("1 conflict"),
            "Error message should contain conflict count: {}",
            error_message
        );
        assert!(
            error_message.contains("unresolved conflicts"),
            "Error message should mention unresolved conflicts: {}",
            error_message
        );
    }

    #[test]
    fn test_error_conflict_report_accessor() {
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

        let write_sets = vec![(CoreId(0), ws0), (CoreId(1), ws1)];

        let result = resolve_conflicts(&write_sets, &ResolutionStrategy::Abort);
        let err = result.unwrap_err();

        // Test the conflict_report() convenience accessor
        let report = err.conflict_report().expect("Should have conflict report");
        assert_eq!(report.len(), 1);
        assert!(report.has_conflicts());
    }

    #[test]
    fn test_error_conflict_report_accessor_returns_none_for_other_errors() {
        // Other error variants should return None from conflict_report()
        let err = crate::Error::NoGroups;
        assert!(err.conflict_report().is_none());

        let err = crate::Error::GroupNotFound(crate::GroupId(42));
        assert!(err.conflict_report().is_none());
    }

    // ========================================================================
    // New API Tests (ConflictTarget, ConflictType)
    // ========================================================================

    #[test]
    fn test_new_api_conflict_target() {
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

        // Use new API to access conflict details
        let conflict = &report.conflicts[0];
        assert!(matches!(
            conflict.target,
            ConflictTarget::GlobalProperty { property: _ }
        ));
        assert_eq!(conflict.conflict_type, ConflictType::WriteWrite);
        assert_eq!(conflict.cores, vec![CoreId(0), CoreId(1)]);

        // Verify target content
        if let ConflictTarget::GlobalProperty { property } = &conflict.target {
            assert_eq!(property, "gold");
        } else {
            panic!("Expected GlobalProperty target");
        }
    }

    #[test]
    fn test_new_api_conflict_target_entity_property() {
        let entity_id = EntityId::new(42);

        let mut ws1 = WriteSet::new();
        ws1.push(PendingWrite::SetProperty {
            entity_id,
            key: "health".to_string(),
            value: Value::Float(100.0),
        });

        let mut ws2 = WriteSet::new();
        ws2.push(PendingWrite::SetProperty {
            entity_id,
            key: "health".to_string(),
            value: Value::Float(50.0),
        });

        let report = detect_conflicts(&[(CoreId(0), ws1), (CoreId(1), ws2)]);
        let conflict = &report.conflicts[0];

        // Use new API
        match &conflict.target {
            ConflictTarget::EntityProperty {
                entity_id: eid,
                property,
            } => {
                assert_eq!(*eid, entity_id);
                assert_eq!(property, "health");
            }
            _ => panic!("Expected EntityProperty target"),
        }
        assert_eq!(conflict.conflict_type, ConflictType::WriteWrite);
    }

    #[test]
    fn test_new_api_conflict_type_display() {
        assert_eq!(format!("{}", ConflictType::WriteWrite), "write-write");
        assert_eq!(format!("{}", ConflictType::ReadWrite), "read-write");
    }

    #[test]
    fn test_new_api_conflict_target_display() {
        let target = ConflictTarget::GlobalProperty {
            property: "gold".to_string(),
        };
        assert_eq!(format!("{}", target), "global 'gold'");

        let target = ConflictTarget::EntityProperty {
            entity_id: EntityId::new(42),
            property: "health".to_string(),
        };
        assert!(format!("{}", target).contains("entity"));
        assert!(format!("{}", target).contains("health"));

        let target = ConflictTarget::SpawnEntity {
            kind: DefId::new("unit"),
        };
        assert!(format!("{}", target).contains("spawn"));
        assert!(format!("{}", target).contains("unit"));
    }

    #[test]
    fn test_new_api_conflict_type_default() {
        let default_type = ConflictType::default();
        assert_eq!(default_type, ConflictType::WriteWrite);
    }

    #[test]
    fn test_new_api_conflict_helper_methods() {
        let conflict = Conflict::new(
            ConflictTarget::GlobalProperty {
                property: "gold".to_string(),
            },
            ConflictType::WriteWrite,
            vec![CoreId(0), CoreId(1)],
            vec![],
        );

        assert!(conflict.is_write_write());
        assert!(!conflict.is_read_write());
        assert_eq!(conflict.core_count(), 2);
    }

    #[test]
    fn test_new_api_resolution_includes_conflict_type() {
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

        let write_sets = vec![(CoreId(0), ws0), (CoreId(1), ws1)];

        let result = resolve_conflicts(&write_sets, &ResolutionStrategy::FirstWriteWins).unwrap();
        assert_eq!(result.resolutions.len(), 1);

        // Check that resolution includes conflict type
        let resolution = &result.resolutions[0];
        assert_eq!(resolution.conflict_type, ConflictType::WriteWrite);
        assert!(matches!(
            resolution.target,
            ConflictTarget::GlobalProperty { .. }
        ));
    }

    #[test]
    fn test_send_sync_for_types() {
        _assert_send_sync::<ConflictTarget>();
        _assert_send_sync::<ConflictType>();
        _assert_send_sync::<ReadRecord>();
    }
}
