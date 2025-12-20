//! Partition Strategies for distributing entities across cores
//!
//! This module provides strategies for partitioning entities across multiple cores
//! in a parallel execution environment. The partitioning is deterministic and
//! seed-configurable, ensuring reproducible results.
//!
//! # Strategies
//!
//! - [`PartitionKind::ById`]: Round-robin partitioning by entity ID (seed-independent)
//! - [`PartitionKind::ByOwner`]: Partition by an owner property value (uses seed)
//! - [`PartitionKind::SpatialGrid`]: 2D spatial grid partitioning (uses seed)
//! - [`PartitionKind::Custom`]: User-defined partitioning function
//!
//! # Seed Configuration
//!
//! Most partition strategies use the hub's deterministic hashing infrastructure.
//! Use [`PartitionStrategy::from_config`] to create strategies that automatically
//! use the hub's configured seed:
//!
//! ```
//! use pulsive_hub::partition::{PartitionStrategy, PartitionKind};
//! use pulsive_hub::HubConfig;
//!
//! let config = HubConfig::with_seed(42);
//!
//! // Create strategy using hub's seed
//! let strategy = PartitionStrategy::from_config(PartitionKind::ByOwner {
//!     property: "owner_id".to_string(),
//! }, &config);
//! assert_eq!(strategy.seed(), 42);
//!
//! // Convenience method
//! let strategy = PartitionStrategy::by_owner_from_config("owner_id", &config);
//! assert_eq!(strategy.seed(), 42);
//! ```
//!
//! **Note:** [`PartitionKind::ById`] uses pure round-robin and does not use the seed.
//! Changing the seed will not affect `ById` partition layouts.
//!
//! # Example
//!
//! ```
//! use pulsive_hub::partition::{PartitionStrategy, PartitionKind, PartitionResult};
//! use pulsive_hub::{HubConfig, DEFAULT_GLOBAL_SEED};
//! use pulsive_core::{Entity, EntityStore, EntityId};
//!
//! // Create an entity store with some entities
//! let mut store = EntityStore::new();
//! for _ in 0..10 {
//!     store.create("unit");
//! }
//!
//! // Using default seed (quick setup)
//! let strategy = PartitionStrategy::by_id();
//! let result = strategy.partition(&store, 4);
//! assert_eq!(result.partition_count(), 4);
//!
//! // Using hub config's seed (recommended for production)
//! let config = HubConfig::with_seed(12345);
//! let strategy = PartitionStrategy::by_owner_from_config("nation_id", &config);
//! ```

use crate::config::hash_seed;
use crate::hash::{hash_u64_with_seed, hash_value_with_seed};
use crate::CoreId;
use crate::HubConfig;
use crate::DEFAULT_GLOBAL_SEED;
use pulsive_core::{Entity, EntityId, EntityStore};
use std::sync::Arc;

/// Type alias for the custom partitioner function
pub type PartitionFn = Arc<dyn Fn(&Entity) -> usize + Send + Sync>;

/// The kind of partitioning strategy to use
#[derive(Clone)]
pub enum PartitionKind {
    /// Round-robin partitioning by entity ID
    ///
    /// Entities are distributed evenly across cores based on their ID:
    /// `core_id = entity_id % core_count`
    ///
    /// **Note:** This strategy does not use the seed. Partition assignments
    /// are purely determined by entity IDs and core count, making them
    /// stable regardless of seed configuration. Use this when you want
    /// consistent, predictable distribution without seed-based variation.
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
    /// The owner value is hashed using the strategy's seed.
    ///
    /// If the owner property is missing, falls back to hashing the entity ID
    /// to avoid hot-spotting all entities to one core.
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

impl std::fmt::Debug for PartitionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PartitionKind::ById => write!(f, "ById"),
            PartitionKind::ByOwner { property } => f
                .debug_struct("ByOwner")
                .field("property", property)
                .finish(),
            PartitionKind::SpatialGrid {
                cell_size,
                x_prop,
                y_prop,
            } => f
                .debug_struct("SpatialGrid")
                .field("cell_size", cell_size)
                .field("x_prop", x_prop)
                .field("y_prop", y_prop)
                .finish(),
            PartitionKind::Custom(_) => write!(f, "Custom(...)"),
        }
    }
}

/// Strategy for partitioning entities across cores
///
/// Combines a [`PartitionKind`] with a seed for deterministic hashing.
/// The seed defaults to [`DEFAULT_GLOBAL_SEED`] but can be customized
/// to produce different partition layouts.
///
/// # Example
///
/// ```
/// use pulsive_hub::partition::{PartitionStrategy, PartitionKind};
/// use pulsive_hub::DEFAULT_GLOBAL_SEED;
///
/// // Default seed
/// let strategy = PartitionStrategy::by_id();
/// assert_eq!(strategy.seed(), DEFAULT_GLOBAL_SEED);
///
/// // Custom seed
/// let strategy = PartitionStrategy::with_seed(PartitionKind::ById, 42);
/// assert_eq!(strategy.seed(), 42);
/// ```
#[derive(Clone)]
pub struct PartitionStrategy {
    /// The kind of partitioning to use
    kind: PartitionKind,
    /// Seed for deterministic hashing
    seed: u64,
}

impl PartitionStrategy {
    /// Create a partition strategy with a specific kind and seed
    ///
    /// # Arguments
    ///
    /// * `kind` - The partitioning algorithm to use
    /// * `seed` - Seed for deterministic hashing
    ///
    /// # Example
    ///
    /// ```
    /// use pulsive_hub::partition::{PartitionStrategy, PartitionKind};
    ///
    /// let strategy = PartitionStrategy::with_seed(PartitionKind::ById, 42);
    /// assert_eq!(strategy.seed(), 42);
    /// ```
    pub fn with_seed(kind: PartitionKind, seed: u64) -> Self {
        Self { kind, seed }
    }

    /// Create a round-robin by-ID partitioning strategy
    ///
    /// Uses [`DEFAULT_GLOBAL_SEED`]. For production code, consider using
    /// [`by_id_from_config`](Self::by_id_from_config) to respect the hub's seed.
    ///
    /// **Note:** The `ById` strategy does not use the seed for partitioning.
    /// See [`PartitionKind::ById`] for details.
    pub fn by_id() -> Self {
        Self::with_seed(PartitionKind::ById, DEFAULT_GLOBAL_SEED)
    }

    /// Create an owner-based partitioning strategy
    ///
    /// Uses [`DEFAULT_GLOBAL_SEED`]. For production code, consider using
    /// [`by_owner_from_config`](Self::by_owner_from_config) to respect the hub's seed.
    ///
    /// # Arguments
    ///
    /// * `property` - The property name containing the owner identifier
    pub fn by_owner(property: impl Into<String>) -> Self {
        Self::with_seed(
            PartitionKind::ByOwner {
                property: property.into(),
            },
            DEFAULT_GLOBAL_SEED,
        )
    }

    /// Create a spatial grid partitioning strategy
    ///
    /// Uses [`DEFAULT_GLOBAL_SEED`]. For production code, consider using
    /// [`spatial_grid_from_config`](Self::spatial_grid_from_config) to respect the hub's seed.
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
        Self::with_seed(
            PartitionKind::SpatialGrid {
                cell_size,
                x_prop: x_prop.into(),
                y_prop: y_prop.into(),
            },
            DEFAULT_GLOBAL_SEED,
        )
    }

    /// Create a custom partitioning strategy
    ///
    /// Uses [`DEFAULT_GLOBAL_SEED`]. For production code, consider using
    /// [`custom_from_config`](Self::custom_from_config) to respect the hub's seed.
    ///
    /// # Arguments
    ///
    /// * `f` - Function that maps an entity to a core index
    pub fn custom<F>(f: F) -> Self
    where
        F: Fn(&Entity) -> usize + Send + Sync + 'static,
    {
        Self::with_seed(PartitionKind::Custom(Arc::new(f)), DEFAULT_GLOBAL_SEED)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Config-based constructors
    // ─────────────────────────────────────────────────────────────────────────

    /// Create a partition strategy using the hub config's global seed
    ///
    /// This is the recommended way to create strategies in production code,
    /// as it ensures the partition layout uses the same seed as the rest of
    /// the hub's deterministic infrastructure.
    ///
    /// # Arguments
    ///
    /// * `kind` - The partitioning algorithm to use
    /// * `config` - Hub configuration to get the seed from
    ///
    /// # Example
    ///
    /// ```
    /// use pulsive_hub::partition::{PartitionStrategy, PartitionKind};
    /// use pulsive_hub::HubConfig;
    ///
    /// let config = HubConfig::with_seed(42);
    /// let strategy = PartitionStrategy::from_config(PartitionKind::ById, &config);
    /// assert_eq!(strategy.seed(), 42);
    /// ```
    pub fn from_config(kind: PartitionKind, config: &HubConfig) -> Self {
        Self::with_seed(kind, config.global_seed())
    }

    /// Create a round-robin by-ID strategy using the hub config's seed
    ///
    /// **Note:** The `ById` strategy does not use the seed for partitioning;
    /// entity assignments are purely based on `entity_id % core_count`.
    /// The seed is stored for consistency but does not affect the layout.
    ///
    /// # Example
    ///
    /// ```
    /// use pulsive_hub::partition::PartitionStrategy;
    /// use pulsive_hub::HubConfig;
    ///
    /// let config = HubConfig::with_seed(42);
    /// let strategy = PartitionStrategy::by_id_from_config(&config);
    /// ```
    pub fn by_id_from_config(config: &HubConfig) -> Self {
        Self::from_config(PartitionKind::ById, config)
    }

    /// Create an owner-based strategy using the hub config's seed
    ///
    /// # Arguments
    ///
    /// * `property` - The property name containing the owner identifier
    /// * `config` - Hub configuration to get the seed from
    ///
    /// # Example
    ///
    /// ```
    /// use pulsive_hub::partition::PartitionStrategy;
    /// use pulsive_hub::HubConfig;
    ///
    /// let config = HubConfig::with_seed(42);
    /// let strategy = PartitionStrategy::by_owner_from_config("nation_id", &config);
    /// assert_eq!(strategy.seed(), 42);
    /// ```
    pub fn by_owner_from_config(property: impl Into<String>, config: &HubConfig) -> Self {
        Self::from_config(
            PartitionKind::ByOwner {
                property: property.into(),
            },
            config,
        )
    }

    /// Create a spatial grid strategy using the hub config's seed
    ///
    /// # Arguments
    ///
    /// * `cell_size` - Size of each grid cell (must be > 0)
    /// * `x_prop` - Property name for the X coordinate
    /// * `y_prop` - Property name for the Y coordinate
    /// * `config` - Hub configuration to get the seed from
    ///
    /// # Panics
    ///
    /// Panics if `cell_size` is <= 0
    ///
    /// # Example
    ///
    /// ```
    /// use pulsive_hub::partition::PartitionStrategy;
    /// use pulsive_hub::HubConfig;
    ///
    /// let config = HubConfig::with_seed(42);
    /// let strategy = PartitionStrategy::spatial_grid_from_config(100.0, "x", "y", &config);
    /// assert_eq!(strategy.seed(), 42);
    /// ```
    pub fn spatial_grid_from_config(
        cell_size: f64,
        x_prop: impl Into<String>,
        y_prop: impl Into<String>,
        config: &HubConfig,
    ) -> Self {
        assert!(cell_size > 0.0, "cell_size must be positive");
        Self::from_config(
            PartitionKind::SpatialGrid {
                cell_size,
                x_prop: x_prop.into(),
                y_prop: y_prop.into(),
            },
            config,
        )
    }

    /// Create a custom partitioning strategy using the hub config's seed
    ///
    /// # Arguments
    ///
    /// * `f` - Function that maps an entity to a core index
    /// * `config` - Hub configuration to get the seed from
    pub fn custom_from_config<F>(f: F, config: &HubConfig) -> Self
    where
        F: Fn(&Entity) -> usize + Send + Sync + 'static,
    {
        Self::from_config(PartitionKind::Custom(Arc::new(f)), config)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Accessors
    // ─────────────────────────────────────────────────────────────────────────

    /// Get the seed used for deterministic hashing
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Get the partition kind
    pub fn kind(&self) -> &PartitionKind {
        &self.kind
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
    ///
    /// # Panics
    ///
    /// Panics if `core_count` is 0.
    pub fn assign_core(&self, entity: &Entity, core_count: usize) -> usize {
        assert!(core_count > 0, "core_count must be at least 1");

        match &self.kind {
            PartitionKind::ById => {
                // Round-robin by entity ID
                entity.id.raw() as usize % core_count
            }

            PartitionKind::ByOwner { property } => {
                // Hash the owner property value to get core assignment
                if let Some(value) = entity.get(property) {
                    let hash = hash_value_with_seed(value, self.seed);
                    hash as usize % core_count
                } else {
                    // Fallback: hash entity ID for balanced distribution
                    // instead of hot-spotting all to core 0
                    hash_u64_with_seed(entity.id.raw(), self.seed) as usize % core_count
                }
            }

            PartitionKind::SpatialGrid {
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

                // Hash the cell coordinates using the seed
                let hash = spatial_hash_with_seed(cell_x, cell_y, self.seed);
                hash as usize % core_count
            }

            PartitionKind::Custom(f) => {
                // Use the custom function, then mod by core_count
                f(entity) % core_count
            }
        }
    }
}

impl std::fmt::Debug for PartitionStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PartitionStrategy")
            .field("kind", &self.kind)
            .field("seed", &self.seed)
            .finish()
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

/// Spatial hash function for 2D grid coordinates with seed
///
/// Uses the hub's `hash_seed` function to combine the seed with
/// spatial coordinates for deterministic, configurable hashing.
fn spatial_hash_with_seed(x: i64, y: i64, seed: u64) -> u64 {
    // Convert to unsigned, handling negative values
    let ux = x.wrapping_add(i64::MAX / 2) as u64;
    let uy = y.wrapping_add(i64::MAX / 2) as u64;

    // Use hash_seed to mix the coordinates with the seed
    // x goes in "core_id" slot, y goes in "tick" slot
    let h = hash_seed(seed, ux, uy);

    // Additional mixing for better distribution
    hash_seed(h, ux ^ uy, 0)
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
    fn test_by_owner_missing_property_fallback() {
        let store = create_test_store(100);

        let strategy = PartitionStrategy::by_owner("owner_id");
        let result = strategy.partition(&store, 4);

        // Entities without owner property should be distributed by ID hash
        // (not all to core 0, which would cause hot-spotting)
        assert_eq!(result.total_entities(), 100);

        // Check that distribution is reasonably balanced (not all in one core)
        let sizes = result.partition_sizes();
        let max_size = *sizes.iter().max().unwrap();
        let min_size = *sizes.iter().min().unwrap();

        // With 100 entities across 4 cores, expect roughly 25 each
        // Allow some variance but ensure it's not all in one partition
        assert!(
            max_size < 50,
            "Missing owner should not hot-spot: max_size={}, expected <50",
            max_size
        );
        assert!(
            min_size > 10,
            "Missing owner should distribute: min_size={}, expected >10",
            min_size
        );
    }

    #[test]
    fn test_by_owner_missing_property_is_deterministic() {
        let store = create_test_store(20);

        let strategy = PartitionStrategy::by_owner("owner_id");
        let result1 = strategy.partition(&store, 4);
        let result2 = strategy.partition(&store, 4);

        // Even with missing owner, should be deterministic
        for i in 0..4 {
            assert_eq!(
                result1.get(CoreId(i)),
                result2.get(CoreId(i)),
                "Missing owner fallback should be deterministic"
            );
        }
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
    // Seed Configuration Tests
    // ========================================================================

    #[test]
    fn test_default_seed() {
        let strategy = PartitionStrategy::by_id();
        assert_eq!(strategy.seed(), DEFAULT_GLOBAL_SEED);
    }

    #[test]
    fn test_with_seed() {
        let strategy = PartitionStrategy::with_seed(PartitionKind::ById, 42);
        assert_eq!(strategy.seed(), 42);
    }

    #[test]
    fn test_different_seeds_affect_hashing() {
        use crate::hash::hash_value_with_seed;
        use pulsive_core::Value;

        // Verify that different seeds produce different hash values
        let value = Value::String("test_owner".into());

        let hash1 = hash_value_with_seed(&value, 12345);
        let hash2 = hash_value_with_seed(&value, 99999);

        assert_ne!(
            hash1, hash2,
            "Different seeds should produce different hash values"
        );

        // Also verify this affects assign_core for the same entity
        let mut store = EntityStore::new();
        let entity = store.create("unit");
        entity.set("owner_id", "test_owner");

        let entity = store.get(EntityId::new(0)).unwrap();

        let strategy1 = PartitionStrategy::with_seed(
            PartitionKind::ByOwner {
                property: "owner_id".to_string(),
            },
            12345,
        );
        let strategy2 = PartitionStrategy::with_seed(
            PartitionKind::ByOwner {
                property: "owner_id".to_string(),
            },
            99999,
        );

        // The raw hash values should differ
        let raw_hash1 = hash_value_with_seed(&Value::String("test_owner".into()), 12345);
        let raw_hash2 = hash_value_with_seed(&Value::String("test_owner".into()), 99999);
        assert_ne!(raw_hash1, raw_hash2, "Raw hashes should differ");

        // Core assignments MAY be the same (modulo can produce same result)
        // but over many cores, they should eventually differ
        let mut found_different = false;
        for core_count in 2..=16 {
            let c1 = strategy1.assign_core(entity, core_count);
            let c2 = strategy2.assign_core(entity, core_count);
            if c1 != c2 {
                found_different = true;
                break;
            }
        }

        assert!(
            found_different,
            "Different seeds should eventually produce different core assignments across various core counts"
        );
    }

    #[test]
    fn test_same_seed_produces_same_partitions() {
        let mut store = EntityStore::new();
        for i in 0..20 {
            let entity = store.create("unit");
            entity.set("owner_id", format!("nation_{}", i % 5));
        }

        let strategy1 = PartitionStrategy::with_seed(
            PartitionKind::ByOwner {
                property: "owner_id".to_string(),
            },
            42,
        );
        let strategy2 = PartitionStrategy::with_seed(
            PartitionKind::ByOwner {
                property: "owner_id".to_string(),
            },
            42,
        );

        let result1 = strategy1.partition(&store, 4);
        let result2 = strategy2.partition(&store, 4);

        // Same seed should produce same layouts
        for i in 0..4 {
            assert_eq!(
                result1.get(CoreId(i)),
                result2.get(CoreId(i)),
                "Same seed should produce same partition layout"
            );
        }
    }

    // ========================================================================
    // Config-based Constructor Tests
    // ========================================================================

    #[test]
    fn test_from_config_uses_hub_seed() {
        let config = HubConfig::with_seed(12345);

        let strategy = PartitionStrategy::from_config(PartitionKind::ById, &config);
        assert_eq!(strategy.seed(), 12345);

        let strategy = PartitionStrategy::by_id_from_config(&config);
        assert_eq!(strategy.seed(), 12345);

        let strategy = PartitionStrategy::by_owner_from_config("owner", &config);
        assert_eq!(strategy.seed(), 12345);

        let strategy = PartitionStrategy::spatial_grid_from_config(100.0, "x", "y", &config);
        assert_eq!(strategy.seed(), 12345);

        let strategy = PartitionStrategy::custom_from_config(|e| e.id.raw() as usize, &config);
        assert_eq!(strategy.seed(), 12345);
    }

    #[test]
    fn test_from_config_matches_with_seed() {
        let config = HubConfig::with_seed(42);

        // Creating via from_config should be equivalent to with_seed
        let via_config = PartitionStrategy::by_owner_from_config("owner_id", &config);
        let via_with_seed = PartitionStrategy::with_seed(
            PartitionKind::ByOwner {
                property: "owner_id".to_string(),
            },
            42,
        );

        assert_eq!(via_config.seed(), via_with_seed.seed());

        // They should produce identical partition results
        let mut store = EntityStore::new();
        for i in 0..20 {
            let entity = store.create("unit");
            entity.set("owner_id", format!("nation_{}", i % 5));
        }

        let result_config = via_config.partition(&store, 4);
        let result_with_seed = via_with_seed.partition(&store, 4);

        for i in 0..4 {
            assert_eq!(
                result_config.get(CoreId(i)),
                result_with_seed.get(CoreId(i))
            );
        }
    }

    #[test]
    fn test_by_id_ignores_seed() {
        // ById should produce the same layout regardless of seed
        let store = create_test_store(20);

        let strategy1 = PartitionStrategy::with_seed(PartitionKind::ById, 1);
        let strategy2 = PartitionStrategy::with_seed(PartitionKind::ById, 99999);

        let result1 = strategy1.partition(&store, 4);
        let result2 = strategy2.partition(&store, 4);

        // Results should be identical because ById uses entity.id % core_count
        for i in 0..4 {
            assert_eq!(
                result1.get(CoreId(i)),
                result2.get(CoreId(i)),
                "ById should ignore seed and produce identical partitions"
            );
        }
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
    fn test_zero_core_count_partition_panics() {
        let store = create_test_store(10);
        let strategy = PartitionStrategy::by_id();
        strategy.partition(&store, 0);
    }

    #[test]
    #[should_panic(expected = "core_count must be at least 1")]
    fn test_zero_core_count_assign_core_panics() {
        let mut store = EntityStore::new();
        store.create("unit");
        let entity = store.get(EntityId::new(0)).unwrap();

        let strategy = PartitionStrategy::by_id();
        // This should panic because core_count is 0
        strategy.assign_core(entity, 0);
    }

    // ========================================================================
    // Debug Implementation Tests
    // ========================================================================

    #[test]
    fn test_debug_strategy() {
        let strategy = PartitionStrategy::by_id();
        let debug = format!("{:?}", strategy);
        assert!(debug.contains("PartitionStrategy"));
        assert!(debug.contains("ById"));
        assert!(debug.contains("seed"));
    }

    #[test]
    fn test_debug_kind_by_owner() {
        let kind = PartitionKind::ByOwner {
            property: "owner_id".to_string(),
        };
        let debug = format!("{:?}", kind);
        assert!(debug.contains("ByOwner"));
        assert!(debug.contains("owner_id"));
    }

    #[test]
    fn test_debug_kind_spatial_grid() {
        let kind = PartitionKind::SpatialGrid {
            cell_size: 100.0,
            x_prop: "x".to_string(),
            y_prop: "y".to_string(),
        };
        let debug = format!("{:?}", kind);
        assert!(debug.contains("SpatialGrid"));
        assert!(debug.contains("100"));
    }

    #[test]
    fn test_debug_kind_custom() {
        let kind = PartitionKind::Custom(Arc::new(|_| 0));
        let debug = format!("{:?}", kind);
        assert!(debug.contains("Custom"));
    }

    // ========================================================================
    // Spatial Hash Tests
    // ========================================================================

    #[test]
    fn test_spatial_hash_deterministic() {
        let h1 = spatial_hash_with_seed(10, 20, DEFAULT_GLOBAL_SEED);
        let h2 = spatial_hash_with_seed(10, 20, DEFAULT_GLOBAL_SEED);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_spatial_hash_different_coords() {
        let h1 = spatial_hash_with_seed(10, 20, DEFAULT_GLOBAL_SEED);
        let h2 = spatial_hash_with_seed(20, 10, DEFAULT_GLOBAL_SEED);
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_spatial_hash_different_seeds() {
        let h1 = spatial_hash_with_seed(10, 20, 100);
        let h2 = spatial_hash_with_seed(10, 20, 200);
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_spatial_hash_negative_coords() {
        // Should not panic and produce valid hashes
        let h1 = spatial_hash_with_seed(-100, -200, DEFAULT_GLOBAL_SEED);
        let h2 = spatial_hash_with_seed(-100, -200, DEFAULT_GLOBAL_SEED);
        assert_eq!(h1, h2);

        // Different negative coords should produce different hashes
        let h3 = spatial_hash_with_seed(-100, -201, DEFAULT_GLOBAL_SEED);
        assert_ne!(h1, h3);
    }
}
