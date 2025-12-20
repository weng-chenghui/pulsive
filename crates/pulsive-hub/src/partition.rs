//! Partition Strategies for distributing entities across cores
//!
//! This module provides strategies for partitioning entities across multiple cores
//! in a parallel execution environment. The partitioning is deterministic to ensure
//! reproducible results regardless of execution order.
//!
//! # Strategies
//!
//! - [`PartitionStrategy::ById`]: Round-robin partitioning by entity ID
//! - [`PartitionStrategy::ByOwner`]: Partition by an owner property value
//! - [`PartitionStrategy::SpatialGrid`]: 2D spatial grid partitioning
//! - [`PartitionStrategy::Custom`]: User-defined partitioning function
//!
//! # Example
//!
//! ```
//! use pulsive_hub::partition::{PartitionStrategy, PartitionResult};
//! use pulsive_core::{Entity, EntityStore, EntityId};
//!
//! // Create an entity store with some entities
//! let mut store = EntityStore::new();
//! for _ in 0..10 {
//!     store.create("unit");
//! }
//!
//! // Partition by ID (round-robin)
//! let strategy = PartitionStrategy::by_id();
//! let result = strategy.partition(&store, 4);
//!
//! assert_eq!(result.partition_count(), 4);
//! assert_eq!(result.total_entities(), 10);
//! ```

use crate::CoreId;
use pulsive_core::{Entity, EntityId, EntityStore};
use std::sync::Arc;

/// Type alias for the custom partitioner function
pub type PartitionFn = Arc<dyn Fn(&Entity) -> usize + Send + Sync>;

/// Strategy for partitioning entities across cores
#[derive(Clone)]
pub enum PartitionStrategy {
    /// Round-robin partitioning by entity ID
    ///
    /// Entities are distributed evenly across cores based on their ID:
    /// `core_id = entity_id % core_count`
    ///
    /// # Example Distribution (4 cores)
    ///
    /// ```text
    /// Core 0: entities 0, 4, 8, 12, ...
    /// Core 1: entities 1, 5, 9, 13, ...
    /// Core 2: entities 2, 6, 10, 14, ...
    /// Core 3: entities 3, 7, 11, 15, ...
    /// ```
    ById,

    /// Partition by an owner property value
    ///
    /// Entities with the same owner value are assigned to the same core.
    /// The owner value is hashed to determine the core assignment.
    ///
    /// # Example
    ///
    /// ```text
    /// // Entities with owner_id = "france" -> Core 0
    /// // Entities with owner_id = "england" -> Core 1
    /// ```
    ByOwner {
        /// The property name to use as the owner key
        property: String,
    },

    /// Spatial grid partitioning for 2D positions
    ///
    /// Divides the world into a grid of cells, with each cell assigned to a core.
    /// Entities are assigned based on which cell their position falls into.
    ///
    /// # Grid Layout Example (3x2 grid = 6 cores)
    ///
    /// ```text
    /// ┌─────┬─────┬─────┐
    /// │ C0  │ C1  │ C2  │
    /// ├─────┼─────┼─────┤
    /// │ C3  │ C4  │ C5  │
    /// └─────┴─────┴─────┘
    /// ```
    SpatialGrid {
        /// Size of each grid cell
        cell_size: f64,
        /// Property name for the X coordinate
        x_prop: String,
        /// Property name for the Y coordinate
        y_prop: String,
    },

    /// Custom partitioning function
    ///
    /// Allows users to provide their own partitioning logic.
    /// The function receives an entity and returns the target core index.
    ///
    /// # Note
    ///
    /// The returned core index will be taken modulo `core_count` to ensure
    /// it's a valid core ID.
    Custom(PartitionFn),
}

impl PartitionStrategy {
    /// Create a round-robin by-ID partitioning strategy
    pub fn by_id() -> Self {
        PartitionStrategy::ById
    }

    /// Create an owner-based partitioning strategy
    ///
    /// # Arguments
    ///
    /// * `property` - The property name containing the owner identifier
    pub fn by_owner(property: impl Into<String>) -> Self {
        PartitionStrategy::ByOwner {
            property: property.into(),
        }
    }

    /// Create a spatial grid partitioning strategy
    ///
    /// # Arguments
    ///
    /// * `cell_size` - Size of each grid cell (must be > 0)
    /// * `x_prop` - Property name for the X coordinate
    /// * `y_prop` - Property name for the Y coordinate
    ///
    /// # Panics
    ///
    /// Panics if `cell_size` is <= 0
    pub fn spatial_grid(
        cell_size: f64,
        x_prop: impl Into<String>,
        y_prop: impl Into<String>,
    ) -> Self {
        assert!(cell_size > 0.0, "cell_size must be positive");
        PartitionStrategy::SpatialGrid {
            cell_size,
            x_prop: x_prop.into(),
            y_prop: y_prop.into(),
        }
    }

    /// Create a custom partitioning strategy
    ///
    /// # Arguments
    ///
    /// * `f` - Function that maps an entity to a core index
    pub fn custom<F>(f: F) -> Self
    where
        F: Fn(&Entity) -> usize + Send + Sync + 'static,
    {
        PartitionStrategy::Custom(Arc::new(f))
    }

    /// Partition entities from a store into groups for each core
    ///
    /// # Arguments
    ///
    /// * `entities` - The entity store containing all entities
    /// * `core_count` - Number of cores to partition across
    ///
    /// # Returns
    ///
    /// A `PartitionResult` containing the entity ID assignments for each core.
    ///
    /// # Panics
    ///
    /// Panics if `core_count` is 0.
    pub fn partition(&self, entities: &EntityStore, core_count: usize) -> PartitionResult {
        assert!(core_count > 0, "core_count must be at least 1");

        // Initialize empty partitions
        let mut partitions: Vec<Vec<EntityId>> = (0..core_count).map(|_| Vec::new()).collect();

        // Assign each entity to a core
        for entity in entities.iter() {
            let core_idx = self.assign_core(entity, core_count);
            partitions[core_idx].push(entity.id);
        }

        PartitionResult {
            partitions,
            core_count,
        }
    }

    /// Assign a single entity to a core
    ///
    /// # Arguments
    ///
    /// * `entity` - The entity to assign
    /// * `core_count` - Number of available cores
    ///
    /// # Returns
    ///
    /// The index of the core this entity should be assigned to (0..core_count)
    pub fn assign_core(&self, entity: &Entity, core_count: usize) -> usize {
        match self {
            PartitionStrategy::ById => {
                // Round-robin by entity ID
                entity.id.raw() as usize % core_count
            }

            PartitionStrategy::ByOwner { property } => {
                // Hash the owner property value to get core assignment
                if let Some(value) = entity.get(property) {
                    let hash = hash_value(value);
                    hash as usize % core_count
                } else {
                    // Entities without the owner property go to core 0
                    0
                }
            }

            PartitionStrategy::SpatialGrid {
                cell_size,
                x_prop,
                y_prop,
            } => {
                // Get position from properties
                let x = entity.get_number(x_prop).unwrap_or(0.0);
                let y = entity.get_number(y_prop).unwrap_or(0.0);

                // Calculate grid cell coordinates
                let cell_x = (x / cell_size).floor() as i64;
                let cell_y = (y / cell_size).floor() as i64;

                // Hash the cell coordinates to get core assignment
                // We use a simple spatial hashing scheme
                let hash = spatial_hash(cell_x, cell_y);
                hash as usize % core_count
            }

            PartitionStrategy::Custom(f) => {
                // Use the custom function, then mod by core_count
                f(entity) % core_count
            }
        }
    }
}

impl std::fmt::Debug for PartitionStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PartitionStrategy::ById => write!(f, "PartitionStrategy::ById"),
            PartitionStrategy::ByOwner { property } => f
                .debug_struct("PartitionStrategy::ByOwner")
                .field("property", property)
                .finish(),
            PartitionStrategy::SpatialGrid {
                cell_size,
                x_prop,
                y_prop,
            } => f
                .debug_struct("PartitionStrategy::SpatialGrid")
                .field("cell_size", cell_size)
                .field("x_prop", x_prop)
                .field("y_prop", y_prop)
                .finish(),
            PartitionStrategy::Custom(_) => write!(f, "PartitionStrategy::Custom(...)"),
        }
    }
}

/// Result of partitioning entities across cores
#[derive(Debug, Clone)]
pub struct PartitionResult {
    /// Entity IDs assigned to each core (indexed by core index)
    partitions: Vec<Vec<EntityId>>,
    /// Number of cores
    core_count: usize,
}

impl PartitionResult {
    /// Get the partition (list of entity IDs) for a specific core
    pub fn get(&self, core_id: CoreId) -> &[EntityId] {
        &self.partitions[core_id.0]
    }

    /// Get all partitions
    pub fn partitions(&self) -> &[Vec<EntityId>] {
        &self.partitions
    }

    /// Get the number of partitions (same as core count)
    pub fn partition_count(&self) -> usize {
        self.core_count
    }

    /// Get the total number of entities across all partitions
    pub fn total_entities(&self) -> usize {
        self.partitions.iter().map(|p| p.len()).sum()
    }

    /// Get the size of each partition
    pub fn partition_sizes(&self) -> Vec<usize> {
        self.partitions.iter().map(|p| p.len()).collect()
    }

    /// Check if partitions are roughly balanced
    ///
    /// Returns true if the difference between the largest and smallest
    /// partition is at most `max_imbalance` (as a ratio of total entities).
    ///
    /// # Arguments
    ///
    /// * `max_imbalance` - Maximum allowed imbalance ratio (e.g., 0.1 for 10%)
    pub fn is_balanced(&self, max_imbalance: f64) -> bool {
        if self.partitions.is_empty() {
            return true;
        }

        let sizes = self.partition_sizes();
        let max_size = *sizes.iter().max().unwrap_or(&0);
        let min_size = *sizes.iter().min().unwrap_or(&0);
        let total = self.total_entities();

        if total == 0 {
            return true;
        }

        let imbalance = (max_size - min_size) as f64 / total as f64;
        imbalance <= max_imbalance
    }

    /// Calculate the imbalance ratio
    ///
    /// Returns the standard deviation of partition sizes divided by the mean.
    /// A value of 0 means perfectly balanced.
    pub fn imbalance_ratio(&self) -> f64 {
        if self.partitions.is_empty() {
            return 0.0;
        }

        let sizes: Vec<f64> = self.partition_sizes().iter().map(|&s| s as f64).collect();
        let n = sizes.len() as f64;
        let mean = sizes.iter().sum::<f64>() / n;

        if mean == 0.0 {
            return 0.0;
        }

        let variance = sizes.iter().map(|&s| (s - mean).powi(2)).sum::<f64>() / n;
        let std_dev = variance.sqrt();

        std_dev / mean
    }

    /// Iterate over partitions with their core IDs
    pub fn iter(&self) -> impl Iterator<Item = (CoreId, &[EntityId])> {
        self.partitions
            .iter()
            .enumerate()
            .map(|(idx, ids)| (CoreId(idx), ids.as_slice()))
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Hash a Value for partitioning purposes
fn hash_value(value: &pulsive_core::Value) -> u64 {
    use pulsive_core::Value;
    use std::hash::{Hash, Hasher};

    // Use a simple but fast hasher
    let mut hasher = std::collections::hash_map::DefaultHasher::new();

    match value {
        Value::Null => 0u8.hash(&mut hasher),
        Value::Bool(b) => b.hash(&mut hasher),
        Value::Int(i) => i.hash(&mut hasher),
        Value::Float(f) => f.to_bits().hash(&mut hasher),
        Value::String(s) => s.hash(&mut hasher),
        Value::EntityRef(id) => id.raw().hash(&mut hasher),
        Value::List(list) => {
            for v in list {
                hasher.write_u64(hash_value(v));
            }
        }
        Value::Map(map) => {
            for (k, v) in map {
                k.hash(&mut hasher);
                hasher.write_u64(hash_value(v));
            }
        }
    }

    hasher.finish()
}

/// Spatial hash function for 2D grid coordinates
///
/// Uses a simple but effective hash that distributes cells evenly.
fn spatial_hash(x: i64, y: i64) -> u64 {
    // Use the Cantor pairing function with wrapping for negative values
    // Then apply a mixing step for better distribution
    let ux = x.wrapping_add(i64::MAX / 2) as u64;
    let uy = y.wrapping_add(i64::MAX / 2) as u64;

    // Mix the coordinates using a simple hash
    let mut hash = ux.wrapping_mul(2654435761);
    hash ^= uy.wrapping_mul(2246822519);
    hash ^= hash >> 16;
    hash = hash.wrapping_mul(0x85ebca6b);
    hash ^= hash >> 13;
    hash
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create a test entity store
    fn create_test_store(count: usize) -> EntityStore {
        let mut store = EntityStore::new();
        for _ in 0..count {
            store.create("unit");
        }
        store
    }

    // ========================================================================
    // ById Partitioning Tests
    // ========================================================================

    #[test]
    fn test_by_id_partitioning() {
        let store = create_test_store(12);
        let strategy = PartitionStrategy::by_id();
        let result = strategy.partition(&store, 4);

        // Check total entities
        assert_eq!(result.total_entities(), 12);
        assert_eq!(result.partition_count(), 4);

        // Each partition should have 3 entities (12 / 4)
        for partition in result.partitions() {
            assert_eq!(partition.len(), 3);
        }
    }

    #[test]
    fn test_by_id_is_deterministic() {
        let store = create_test_store(100);
        let strategy = PartitionStrategy::by_id();

        let result1 = strategy.partition(&store, 4);
        let result2 = strategy.partition(&store, 4);

        // Same inputs should produce same outputs
        for i in 0..4 {
            assert_eq!(
                result1.get(CoreId(i)),
                result2.get(CoreId(i)),
                "Partition {} should be deterministic",
                i
            );
        }
    }

    #[test]
    fn test_by_id_round_robin_pattern() {
        let store = create_test_store(8);
        let strategy = PartitionStrategy::by_id();
        let result = strategy.partition(&store, 4);

        // Verify round-robin pattern
        // Entity 0 -> Core 0, Entity 1 -> Core 1, Entity 4 -> Core 0, etc.
        assert!(result.get(CoreId(0)).contains(&EntityId::new(0)));
        assert!(result.get(CoreId(1)).contains(&EntityId::new(1)));
        assert!(result.get(CoreId(2)).contains(&EntityId::new(2)));
        assert!(result.get(CoreId(3)).contains(&EntityId::new(3)));
        assert!(result.get(CoreId(0)).contains(&EntityId::new(4)));
    }

    // ========================================================================
    // ByOwner Partitioning Tests
    // ========================================================================

    #[test]
    fn test_by_owner_partitioning() {
        let mut store = EntityStore::new();

        // Create entities with different owners
        for i in 0..12 {
            let entity = store.create("unit");
            let owner = match i % 3 {
                0 => "france",
                1 => "england",
                _ => "spain",
            };
            entity.set("owner_id", owner);
        }

        let strategy = PartitionStrategy::by_owner("owner_id");
        let result = strategy.partition(&store, 4);

        // All entities with same owner should be in the same partition
        // (though which partition depends on the hash)
        assert_eq!(result.total_entities(), 12);
    }

    #[test]
    fn test_by_owner_same_owner_same_partition() {
        let mut store = EntityStore::new();

        // Create 10 entities all owned by "france"
        for _ in 0..10 {
            let entity = store.create("unit");
            entity.set("owner_id", "france");
        }

        let strategy = PartitionStrategy::by_owner("owner_id");
        let result = strategy.partition(&store, 4);

        // All entities should be in the same partition
        let non_empty: Vec<_> = result
            .partitions()
            .iter()
            .filter(|p| !p.is_empty())
            .collect();
        assert_eq!(
            non_empty.len(),
            1,
            "All entities with same owner should be in one partition"
        );
        assert_eq!(non_empty[0].len(), 10);
    }

    #[test]
    fn test_by_owner_is_deterministic() {
        let mut store = EntityStore::new();
        for i in 0..20 {
            let entity = store.create("unit");
            entity.set("owner_id", format!("nation_{}", i % 5));
        }

        let strategy = PartitionStrategy::by_owner("owner_id");
        let result1 = strategy.partition(&store, 4);
        let result2 = strategy.partition(&store, 4);

        for i in 0..4 {
            assert_eq!(result1.get(CoreId(i)), result2.get(CoreId(i)));
        }
    }

    #[test]
    fn test_by_owner_missing_property() {
        let store = create_test_store(5);

        let strategy = PartitionStrategy::by_owner("owner_id");
        let result = strategy.partition(&store, 4);

        // All entities without owner property should go to core 0
        assert_eq!(result.get(CoreId(0)).len(), 5);
    }

    // ========================================================================
    // SpatialGrid Partitioning Tests
    // ========================================================================

    #[test]
    fn test_spatial_grid_partitioning() {
        let mut store = EntityStore::new();

        // Create entities in a 3x3 grid pattern
        for x in 0..3 {
            for y in 0..3 {
                let entity = store.create("unit");
                entity.set("x", (x * 100) as f64);
                entity.set("y", (y * 100) as f64);
            }
        }

        let strategy = PartitionStrategy::spatial_grid(100.0, "x", "y");
        let result = strategy.partition(&store, 4);

        assert_eq!(result.total_entities(), 9);
    }

    #[test]
    fn test_spatial_grid_same_cell_same_partition() {
        let mut store = EntityStore::new();

        // Create 10 entities all in the same grid cell
        for i in 0..10 {
            let entity = store.create("unit");
            entity.set("x", 50.0 + i as f64); // All within cell (0, 0) if cell_size = 100
            entity.set("y", 50.0 + i as f64);
        }

        let strategy = PartitionStrategy::spatial_grid(100.0, "x", "y");
        let result = strategy.partition(&store, 4);

        // All entities should be in the same partition
        let non_empty: Vec<_> = result
            .partitions()
            .iter()
            .filter(|p| !p.is_empty())
            .collect();
        assert_eq!(
            non_empty.len(),
            1,
            "All entities in same cell should be in one partition"
        );
    }

    #[test]
    fn test_spatial_grid_is_deterministic() {
        let mut store = EntityStore::new();
        for i in 0..20 {
            let entity = store.create("unit");
            entity.set("x", (i * 50) as f64);
            entity.set("y", ((i * 73) % 500) as f64);
        }

        let strategy = PartitionStrategy::spatial_grid(100.0, "x", "y");
        let result1 = strategy.partition(&store, 4);
        let result2 = strategy.partition(&store, 4);

        for i in 0..4 {
            assert_eq!(result1.get(CoreId(i)), result2.get(CoreId(i)));
        }
    }

    #[test]
    fn test_spatial_grid_negative_coordinates() {
        let mut store = EntityStore::new();

        // Create entities with negative coordinates
        for x in -2..=2 {
            for y in -2..=2 {
                let entity = store.create("unit");
                entity.set("x", (x * 100) as f64);
                entity.set("y", (y * 100) as f64);
            }
        }

        let strategy = PartitionStrategy::spatial_grid(100.0, "x", "y");
        let result = strategy.partition(&store, 4);

        assert_eq!(result.total_entities(), 25);
    }

    #[test]
    #[should_panic(expected = "cell_size must be positive")]
    fn test_spatial_grid_invalid_cell_size() {
        PartitionStrategy::spatial_grid(0.0, "x", "y");
    }

    // ========================================================================
    // Custom Partitioning Tests
    // ========================================================================

    #[test]
    fn test_custom_partitioning() {
        let store = create_test_store(10);

        // Custom strategy: all entities go to core 2
        let strategy = PartitionStrategy::custom(|_| 2);
        let result = strategy.partition(&store, 4);

        assert_eq!(result.get(CoreId(2)).len(), 10);
        assert_eq!(result.get(CoreId(0)).len(), 0);
        assert_eq!(result.get(CoreId(1)).len(), 0);
        assert_eq!(result.get(CoreId(3)).len(), 0);
    }

    #[test]
    fn test_custom_partitioning_overflow_handled() {
        let store = create_test_store(10);

        // Custom strategy returns a value larger than core_count
        // Should be handled with modulo
        let strategy = PartitionStrategy::custom(|e| e.id.raw() as usize + 1000);
        let result = strategy.partition(&store, 4);

        // All entities should still be distributed (modulo 4)
        assert_eq!(result.total_entities(), 10);
    }

    #[test]
    fn test_custom_partitioning_by_kind() {
        let mut store = EntityStore::new();
        for _ in 0..5 {
            store.create("infantry");
        }
        for _ in 0..5 {
            store.create("cavalry");
        }

        // Partition by entity kind
        let strategy =
            PartitionStrategy::custom(|e| if e.kind.as_str() == "infantry" { 0 } else { 1 });
        let result = strategy.partition(&store, 4);

        // Infantry goes to core 0, cavalry to core 1
        assert_eq!(result.get(CoreId(0)).len(), 5);
        assert_eq!(result.get(CoreId(1)).len(), 5);
    }

    // ========================================================================
    // PartitionResult Tests
    // ========================================================================

    #[test]
    fn test_partition_result_is_balanced() {
        let store = create_test_store(100);
        let strategy = PartitionStrategy::by_id();
        let result = strategy.partition(&store, 4);

        // Round-robin should be perfectly balanced
        assert!(result.is_balanced(0.01));
    }

    #[test]
    fn test_partition_result_imbalance_ratio() {
        let store = create_test_store(100);
        let strategy = PartitionStrategy::by_id();
        let result = strategy.partition(&store, 4);

        // Round-robin should have very low imbalance
        assert!(result.imbalance_ratio() < 0.01);
    }

    #[test]
    fn test_partition_result_iter() {
        let store = create_test_store(8);
        let strategy = PartitionStrategy::by_id();
        let result = strategy.partition(&store, 4);

        let iterated: Vec<_> = result.iter().collect();
        assert_eq!(iterated.len(), 4);

        for (core_id, entities) in iterated {
            assert_eq!(entities.len(), 2);
            assert!(core_id.0 < 4);
        }
    }

    #[test]
    fn test_empty_store_partitioning() {
        let store = EntityStore::new();
        let strategy = PartitionStrategy::by_id();
        let result = strategy.partition(&store, 4);

        assert_eq!(result.total_entities(), 0);
        assert_eq!(result.partition_count(), 4);
        assert!(result.is_balanced(0.0));
    }

    #[test]
    fn test_single_core_partitioning() {
        let store = create_test_store(10);
        let strategy = PartitionStrategy::by_id();
        let result = strategy.partition(&store, 1);

        assert_eq!(result.partition_count(), 1);
        assert_eq!(result.get(CoreId(0)).len(), 10);
    }

    #[test]
    #[should_panic(expected = "core_count must be at least 1")]
    fn test_zero_core_count_panics() {
        let store = create_test_store(10);
        let strategy = PartitionStrategy::by_id();
        strategy.partition(&store, 0);
    }

    // ========================================================================
    // Debug Implementation Tests
    // ========================================================================

    #[test]
    fn test_debug_by_id() {
        let strategy = PartitionStrategy::by_id();
        let debug = format!("{:?}", strategy);
        assert!(debug.contains("ById"));
    }

    #[test]
    fn test_debug_by_owner() {
        let strategy = PartitionStrategy::by_owner("owner_id");
        let debug = format!("{:?}", strategy);
        assert!(debug.contains("ByOwner"));
        assert!(debug.contains("owner_id"));
    }

    #[test]
    fn test_debug_spatial_grid() {
        let strategy = PartitionStrategy::spatial_grid(100.0, "x", "y");
        let debug = format!("{:?}", strategy);
        assert!(debug.contains("SpatialGrid"));
        assert!(debug.contains("100"));
    }

    #[test]
    fn test_debug_custom() {
        let strategy = PartitionStrategy::custom(|_| 0);
        let debug = format!("{:?}", strategy);
        assert!(debug.contains("Custom"));
    }

    // ========================================================================
    // Hash Function Tests
    // ========================================================================

    #[test]
    fn test_hash_value_deterministic() {
        use pulsive_core::Value;

        let v1 = Value::String("test".into());
        let v2 = Value::String("test".into());

        assert_eq!(hash_value(&v1), hash_value(&v2));
    }

    #[test]
    fn test_hash_value_different_for_different_values() {
        use pulsive_core::Value;

        let v1 = Value::String("france".into());
        let v2 = Value::String("england".into());

        assert_ne!(hash_value(&v1), hash_value(&v2));
    }

    #[test]
    fn test_spatial_hash_deterministic() {
        assert_eq!(spatial_hash(10, 20), spatial_hash(10, 20));
        assert_ne!(spatial_hash(10, 20), spatial_hash(20, 10));
    }

    #[test]
    fn test_spatial_hash_negative_coords() {
        // Should not panic and produce valid hashes
        let h1 = spatial_hash(-100, -200);
        let h2 = spatial_hash(-100, -200);
        assert_eq!(h1, h2);

        // Different negative coords should produce different hashes
        let h3 = spatial_hash(-100, -201);
        assert_ne!(h1, h3);
    }
}
