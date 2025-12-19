//! ModelSnapshot - Immutable view of the model for parallel reads
//!
//! Provides thread-safe, immutable snapshots of the model state that can be
//! shared across multiple cores for parallel execution.
//!
//! # Design
//!
//! Snapshots use `Arc` for efficient structural sharing:
//! - **Creating a snapshot is O(1)**: Just clones Arc references from Model
//! - **Cloning a snapshot is O(1)**: Increments Arc reference counts
//! - **Multiple snapshots share data**: Until Model is mutated (copy-on-write)
//!
//! Model stores its entities and globals in `Arc`, so snapshot creation is just
//! cloning the Arc references - no data copying occurs.
//!
//! # Example
//!
//! ```rust,ignore
//! use pulsive_hub::{Hub, ModelSnapshot};
//!
//! let hub = Hub::new();
//! let snapshot = hub.snapshot();
//!
//! // Read entities
//! if let Some(entity) = snapshot.get_entity(entity_id) {
//!     println!("Entity gold: {:?}", entity.get_number("gold"));
//! }
//!
//! // Read globals
//! if let Some(value) = snapshot.get_global("game_speed") {
//!     println!("Game speed: {:?}", value);
//! }
//! ```

use pulsive_core::{
    ActorId, Clock, Context, DefId, Entity, EntityId, EntityStore, IndexMap, Model, Rng, Value,
    ValueMap,
};
use std::sync::Arc;

/// An immutable snapshot of the model at a point in time
///
/// Used to provide consistent read access to cores during parallel execution.
/// Each core receives a snapshot and can read from it without synchronization.
///
/// # Full State Preservation
///
/// The snapshot preserves *all* model state for deterministic replay:
/// - Entities and globals (Arc-wrapped for O(1) cloning)
/// - Clock state (tick, speed, start date)
/// - RNG state (for deterministic random number generation)
/// - Actor contexts
///
/// # Performance
///
/// - **Creation**: O(1) for entities/globals (Arc clone), O(n) for actors
/// - **Clone**: O(1) for entities/globals, O(n) for actors
/// - **Read**: O(1) - direct access to underlying data
///
/// # Thread Safety
///
/// `ModelSnapshot` is `Send + Sync` (auto-derived via Arc), allowing safe
/// sharing across threads for parallel reads.
#[derive(Debug, Clone)]
pub struct ModelSnapshot {
    /// Shared entity storage
    entities: Arc<EntityStore>,
    /// Shared global properties
    globals: Arc<ValueMap>,
    /// Clock state (for full time restoration)
    time: Clock,
    /// RNG state (for deterministic replay)
    rng: Rng,
    /// Actor contexts
    actors: IndexMap<ActorId, Context>,
    /// Version number (for MVCC)
    version: u64,
}

impl ModelSnapshot {
    /// Create a new snapshot from a model
    ///
    /// Captures the full model state for deterministic replay:
    /// - Entities and globals: O(1) Arc clone
    /// - Clock, RNG, actors: O(n) clone
    pub fn new(model: &Model, version: u64) -> Self {
        Self {
            // O(1) - just clone Arc references
            entities: model.entities_arc(),
            globals: model.globals_arc(),
            // Full state for deterministic replay
            time: model.clock().clone(),
            rng: model.rng().clone(),
            actors: model.actors().clone(),
            version,
        }
    }

    /// Create a snapshot with pre-wrapped Arc data
    ///
    /// Used internally when the caller already has Arc-wrapped data.
    #[allow(dead_code)] // Will be used in future optimizations
    pub(crate) fn from_arcs(
        entities: Arc<EntityStore>,
        globals: Arc<ValueMap>,
        time: Clock,
        rng: Rng,
        actors: IndexMap<ActorId, Context>,
        version: u64,
    ) -> Self {
        Self {
            entities,
            globals,
            time,
            rng,
            actors,
            version,
        }
    }

    /// Get the tick this snapshot was taken at
    pub fn tick(&self) -> u64 {
        self.time.tick
    }

    /// Get the clock state
    pub fn clock(&self) -> &Clock {
        &self.time
    }

    /// Get the RNG state
    pub fn rng(&self) -> &Rng {
        &self.rng
    }

    /// Get the actor contexts
    pub fn actors(&self) -> &IndexMap<ActorId, Context> {
        &self.actors
    }

    /// Get the version number
    pub fn version(&self) -> u64 {
        self.version
    }

    // ========================================================================
    // Entity Read Methods
    // ========================================================================

    /// Get an entity by ID
    ///
    /// Returns an immutable reference to the entity if it exists.
    pub fn get_entity(&self, id: EntityId) -> Option<&Entity> {
        self.entities.get(id)
    }

    /// Iterate over all entities of a given kind
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// for entity in snapshot.entities_by_kind(&DefId::new("nation")) {
    ///     println!("{}: gold = {:?}", entity.id, entity.get_number("gold"));
    /// }
    /// ```
    pub fn entities_by_kind(&self, kind: &DefId) -> impl Iterator<Item = &Entity> {
        self.entities.by_kind(kind)
    }

    /// Iterate over all entities
    pub fn entities(&self) -> impl Iterator<Item = &Entity> {
        self.entities.iter()
    }

    /// Get the number of entities
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    /// Check if an entity exists
    pub fn has_entity(&self, id: EntityId) -> bool {
        self.entities.get(id).is_some()
    }

    // ========================================================================
    // Global Read Methods
    // ========================================================================

    /// Get a global property value
    ///
    /// Returns an immutable reference to the value if it exists.
    pub fn get_global(&self, key: &str) -> Option<&Value> {
        self.globals.get(key)
    }

    /// Get a global property as a float
    pub fn get_global_number(&self, key: &str) -> Option<f64> {
        self.globals.get(key).and_then(|v| v.as_float())
    }

    /// Get a global property as a string
    pub fn get_global_str(&self, key: &str) -> Option<&str> {
        self.globals.get(key).and_then(|v| v.as_str())
    }

    /// Check if a global property exists
    pub fn has_global(&self, key: &str) -> bool {
        self.globals.contains_key(key)
    }

    /// Iterate over all global properties
    pub fn globals_iter(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.globals.iter()
    }

    // ========================================================================
    // Conversion Methods
    // ========================================================================

    /// Convert to an owned Model for a core to use
    ///
    /// Each core gets its own mutable copy to work with.
    /// This clones all state including entities, globals, RNG, and actors
    /// for deterministic replay/parallel execution.
    pub fn to_model(&self) -> Model {
        Model::from_snapshot_data(
            (*self.entities).clone(),
            (*self.globals).clone(),
            self.time.clone(),
            self.rng.clone(),
            self.actors.clone(),
        )
    }

    /// Get a reference to the underlying entity store
    ///
    /// Useful when you need direct access to EntityStore methods.
    pub fn entity_store(&self) -> &EntityStore {
        &self.entities
    }

    /// Get a reference to the underlying globals map
    pub fn globals_map(&self) -> &ValueMap {
        &self.globals
    }

    /// Get the Arc-wrapped entity store (for efficient sharing)
    pub fn entities_arc(&self) -> Arc<EntityStore> {
        Arc::clone(&self.entities)
    }

    /// Get the Arc-wrapped globals map (for efficient sharing)
    pub fn globals_arc(&self) -> Arc<ValueMap> {
        Arc::clone(&self.globals)
    }
}

// ModelSnapshot is automatically Send + Sync because:
// - Arc<T> is Send + Sync when T: Send + Sync
// - EntityStore and ValueMap are both Send + Sync
// No unsafe impl needed - compiler derives these automatically.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_creation() {
        let mut model = Model::new();
        model.set_global("gold", 100.0f64);
        model.advance_tick();
        model.advance_tick();

        let snapshot = ModelSnapshot::new(&model, 1);

        assert_eq!(snapshot.tick(), 2);
        assert_eq!(snapshot.version(), 1);
    }

    #[test]
    fn test_snapshot_read_global() {
        let mut model = Model::new();
        model.set_global("gold", 100.0f64);
        model.set_global("name", "TestGame");

        let snapshot = ModelSnapshot::new(&model, 1);

        assert_eq!(snapshot.get_global_number("gold"), Some(100.0));
        assert_eq!(snapshot.get_global_str("name"), Some("TestGame"));
        assert!(snapshot.has_global("gold"));
        assert!(!snapshot.has_global("silver"));
    }

    #[test]
    fn test_snapshot_read_entity() {
        let mut model = Model::new();
        let entity = model.entities_mut().create("nation");
        entity.set("name", "France");
        entity.set("gold", 500.0f64);
        let entity_id = entity.id;

        let snapshot = ModelSnapshot::new(&model, 1);

        let entity = snapshot.get_entity(entity_id).unwrap();
        assert_eq!(entity.get("name").and_then(|v| v.as_str()), Some("France"));
        assert_eq!(entity.get_number("gold"), Some(500.0));
        assert!(snapshot.has_entity(entity_id));
    }

    #[test]
    fn test_snapshot_entities_by_kind() {
        let mut model = Model::new();
        model.entities_mut().create("nation").set("name", "France");
        model.entities_mut().create("nation").set("name", "England");
        model.entities_mut().create("province").set("name", "Paris");

        let snapshot = ModelSnapshot::new(&model, 1);

        assert_eq!(snapshot.entities_by_kind(&DefId::new("nation")).count(), 2);
        assert_eq!(
            snapshot.entities_by_kind(&DefId::new("province")).count(),
            1
        );
        assert_eq!(snapshot.entity_count(), 3);
    }

    #[test]
    fn test_snapshot_immutability() {
        let mut model = Model::new();
        model.set_global("gold", 100.0f64);

        let snapshot = ModelSnapshot::new(&model, 1);

        // Modify the original model
        model.set_global("gold", 200.0f64);

        // Snapshot should still have original value
        assert_eq!(snapshot.get_global_number("gold"), Some(100.0));
    }

    #[test]
    fn test_snapshot_to_model() {
        let mut model = Model::new();
        model.set_global("gold", 100.0f64);
        model.entities_mut().create("nation").set("name", "France");
        model.advance_tick();
        model.advance_tick();
        model.advance_tick();

        let snapshot = ModelSnapshot::new(&model, 1);
        let restored = snapshot.to_model();

        assert_eq!(restored.current_tick(), 3);
        assert_eq!(
            restored.get_global("gold").and_then(|v| v.as_float()),
            Some(100.0)
        );
        assert_eq!(restored.entities().len(), 1);
    }

    #[test]
    fn test_snapshot_arc_sharing() {
        let mut model = Model::new();
        model.set_global("gold", 100.0f64);

        let snapshot1 = ModelSnapshot::new(&model, 1);
        let snapshot2 = snapshot1.clone();

        // Both snapshots share the same underlying data
        assert!(Arc::ptr_eq(&snapshot1.entities, &snapshot2.entities));
        assert!(Arc::ptr_eq(&snapshot1.globals, &snapshot2.globals));
    }

    #[test]
    fn test_snapshot_o1_creation() {
        // Verify that snapshot creation is O(1) by checking Arc sharing with Model
        let mut model = Model::new();
        model.set_global("gold", 100.0f64);
        model.entities_mut().create("nation").set("name", "France");

        // Get Arc references from model before creating snapshot
        let model_entities_arc = model.entities_arc();
        let model_globals_arc = model.globals_arc();

        // Create snapshot - should share the same Arcs (O(1), no data copy)
        let snapshot = ModelSnapshot::new(&model, 1);

        // Snapshot Arcs should point to same data as Model Arcs
        assert!(Arc::ptr_eq(&model_entities_arc, &snapshot.entities_arc()));
        assert!(Arc::ptr_eq(&model_globals_arc, &snapshot.globals_arc()));

        // Multiple snapshots also share the same data
        let snapshot2 = ModelSnapshot::new(&model, 2);
        assert!(Arc::ptr_eq(
            &snapshot.entities_arc(),
            &snapshot2.entities_arc()
        ));
    }

    #[test]
    fn test_snapshot_isolation_after_mutation() {
        // Verify that mutations don't affect existing snapshots (COW)
        let mut model = Model::new();
        model.set_global("gold", 100.0f64);

        let snapshot = ModelSnapshot::new(&model, 1);

        // Mutate model - triggers copy-on-write
        model.set_global("gold", 200.0f64);

        // Snapshot should still have old value
        assert_eq!(snapshot.get_global_number("gold"), Some(100.0));

        // Model has new value
        assert_eq!(
            model.get_global("gold").and_then(|v| v.as_float()),
            Some(200.0)
        );

        // Arcs should now be different (COW triggered)
        assert!(!Arc::ptr_eq(&model.globals_arc(), &snapshot.globals_arc()));
    }

    #[test]
    fn test_snapshot_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ModelSnapshot>();
    }

    #[test]
    fn test_snapshot_globals_iteration() {
        let mut model = Model::new();
        model.set_global("gold", 100.0f64);
        model.set_global("silver", 50.0f64);

        let snapshot = ModelSnapshot::new(&model, 1);

        let keys: Vec<_> = snapshot.globals_iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&"gold"));
        assert!(keys.contains(&"silver"));
    }
}
