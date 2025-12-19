//! System model (state)
//!
//! The Model uses `Arc` for structural sharing, enabling O(1) snapshot creation.
//! Mutations use copy-on-write semantics via `Arc::make_mut()`.

use crate::{ActorId, Clock, Context, EntityStore, Rng, Value, ValueMap};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// The complete system state
///
/// Uses `Arc` for entities and globals to enable efficient snapshotting:
/// - Snapshot creation is O(1) (just Arc clone)
/// - Mutations use copy-on-write (only clones if shared)
/// - Multiple snapshots can share underlying data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    /// All entities in the system (Arc for structural sharing)
    #[serde(with = "arc_entity_store")]
    entities: Arc<EntityStore>,
    /// Global properties (Arc for structural sharing)
    #[serde(with = "arc_value_map")]
    globals: Arc<ValueMap>,
    /// System clock
    pub time: Clock,
    /// Deterministic RNG
    pub rng: Rng,
    /// Actor contexts
    pub actors: IndexMap<ActorId, Context>,
}

// Custom serde for Arc<EntityStore>
mod arc_entity_store {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S>(data: &Arc<EntityStore>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        data.as_ref().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Arc<EntityStore>, D::Error>
    where
        D: Deserializer<'de>,
    {
        EntityStore::deserialize(deserializer).map(Arc::new)
    }
}

// Custom serde for Arc<ValueMap>
mod arc_value_map {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S>(data: &Arc<ValueMap>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        data.as_ref().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Arc<ValueMap>, D::Error>
    where
        D: Deserializer<'de>,
    {
        ValueMap::deserialize(deserializer).map(Arc::new)
    }
}

impl Model {
    /// Create a new empty model
    pub fn new() -> Self {
        Self {
            entities: Arc::new(EntityStore::new()),
            globals: Arc::new(ValueMap::new()),
            time: Clock::new(),
            rng: Rng::new(12345),
            actors: IndexMap::new(),
        }
    }

    /// Create with a specific RNG seed
    pub fn with_seed(seed: u64) -> Self {
        Self {
            entities: Arc::new(EntityStore::new()),
            globals: Arc::new(ValueMap::new()),
            time: Clock::new(),
            rng: Rng::new(seed),
            actors: IndexMap::new(),
        }
    }

    /// Create a Model from snapshot data
    ///
    /// Used when reconstructing a Model from a snapshot. All fields are
    /// restored to match the original model state.
    pub fn from_snapshot_data(
        entities: EntityStore,
        globals: ValueMap,
        time: Clock,
        rng: Rng,
        actors: IndexMap<ActorId, Context>,
    ) -> Self {
        Self {
            entities: Arc::new(entities),
            globals: Arc::new(globals),
            time,
            rng,
            actors,
        }
    }

    // ========================================================================
    // Entity Access
    // ========================================================================

    /// Get a reference to the entity store (for reading)
    pub fn entities(&self) -> &EntityStore {
        &self.entities
    }

    /// Get a mutable reference to the entity store (copy-on-write)
    ///
    /// Uses `Arc::make_mut()` for copy-on-write semantics:
    /// - If this is the only reference, mutates in place
    /// - If shared with snapshots, clones before mutating
    pub fn entities_mut(&mut self) -> &mut EntityStore {
        Arc::make_mut(&mut self.entities)
    }

    /// Get the Arc-wrapped entity store (for O(1) snapshot creation)
    pub fn entities_arc(&self) -> Arc<EntityStore> {
        Arc::clone(&self.entities)
    }

    // ========================================================================
    // Global Property Access
    // ========================================================================

    /// Get a reference to the globals map (for reading)
    pub fn globals(&self) -> &ValueMap {
        &self.globals
    }

    /// Get a mutable reference to the globals map (copy-on-write)
    pub fn globals_mut(&mut self) -> &mut ValueMap {
        Arc::make_mut(&mut self.globals)
    }

    /// Get the Arc-wrapped globals map (for O(1) snapshot creation)
    pub fn globals_arc(&self) -> Arc<ValueMap> {
        Arc::clone(&self.globals)
    }

    /// Get a global property
    pub fn get_global(&self, key: &str) -> Option<&Value> {
        self.globals.get(key)
    }

    /// Set a global property (triggers copy-on-write if shared)
    pub fn set_global(&mut self, key: impl Into<String>, value: impl Into<Value>) {
        Arc::make_mut(&mut self.globals).insert(key.into(), value.into());
    }

    // ========================================================================
    // Actor Management
    // ========================================================================

    /// Add an actor
    pub fn add_actor(&mut self, actor: Context) {
        self.actors.insert(actor.id, actor);
    }

    /// Get an actor context
    pub fn get_actor(&self, id: ActorId) -> Option<&Context> {
        self.actors.get(&id)
    }

    /// Get a mutable actor context
    pub fn get_actor_mut(&mut self, id: ActorId) -> Option<&mut Context> {
        self.actors.get_mut(&id)
    }

    /// Get all actors
    pub fn actors(&self) -> &IndexMap<ActorId, Context> {
        &self.actors
    }

    // ========================================================================
    // Time Management
    // ========================================================================

    /// Get the clock
    pub fn clock(&self) -> &Clock {
        &self.time
    }

    /// Advance the clock by one tick
    pub fn advance_tick(&mut self) {
        self.time.advance();
    }

    /// Get the current tick
    pub fn current_tick(&self) -> u64 {
        self.time.tick
    }

    // ========================================================================
    // RNG Access
    // ========================================================================

    /// Get the RNG state (for snapshotting)
    pub fn rng(&self) -> &Rng {
        &self.rng
    }

    // ========================================================================
    // Evaluation Context Support
    // ========================================================================

    /// Get entities, globals, and rng references for creating an EvalContext
    ///
    /// This method allows simultaneous borrowing of entities/globals (immutable)
    /// and rng (mutable) which is needed for expression evaluation.
    ///
    /// # Returns
    ///
    /// A tuple of `(&EntityStore, &ValueMap, &mut Rng)` that can be passed
    /// to `EvalContext::new()`.
    pub fn eval_refs(&mut self) -> (&EntityStore, &ValueMap, &mut Rng) {
        (&self.entities, &self.globals, &mut self.rng)
    }
}

impl Default for Model {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_globals() {
        let mut model = Model::new();
        model.set_global("difficulty", 2i64);

        assert_eq!(model.get_global("difficulty"), Some(&Value::Int(2)));
    }

    #[test]
    fn test_model_actors() {
        let mut model = Model::new();
        let actor = Context::new(ActorId::new(1));
        model.add_actor(actor);

        assert!(model.get_actor(ActorId::new(1)).is_some());
        assert!(model.get_actor(ActorId::new(2)).is_none());
    }

    #[test]
    fn test_model_arc_sharing() {
        let mut model = Model::new();
        model.set_global("gold", 100.0f64);

        // Get Arc references before mutation
        let entities_arc = model.entities_arc();
        let globals_arc = model.globals_arc();

        // Clone model (simulates snapshot creation)
        let snapshot = model.clone();

        // Arcs should be shared (same pointer)
        assert!(Arc::ptr_eq(&entities_arc, &snapshot.entities_arc()));
        assert!(Arc::ptr_eq(&globals_arc, &snapshot.globals_arc()));

        // Mutate original - should trigger copy-on-write
        model.set_global("gold", 200.0f64);

        // Now they should be different
        assert!(!Arc::ptr_eq(&model.globals_arc(), &snapshot.globals_arc()));

        // But entities should still be shared (no entity mutation)
        assert!(Arc::ptr_eq(&model.entities_arc(), &snapshot.entities_arc()));

        // Values should be independent
        assert_eq!(
            model.get_global("gold").and_then(|v| v.as_float()),
            Some(200.0)
        );
        assert_eq!(
            snapshot.get_global("gold").and_then(|v| v.as_float()),
            Some(100.0)
        );
    }

    #[test]
    fn test_model_entity_cow() {
        let mut model = Model::new();
        model.entities_mut().create("nation").set("name", "France");

        // Clone model
        let snapshot = model.clone();

        // Arcs should be shared
        assert!(Arc::ptr_eq(&model.entities_arc(), &snapshot.entities_arc()));

        // Mutate original entity store
        model.entities_mut().create("nation").set("name", "England");

        // Now they should be different (copy-on-write triggered)
        assert!(!Arc::ptr_eq(
            &model.entities_arc(),
            &snapshot.entities_arc()
        ));

        // Original has 2 nations, snapshot has 1
        assert_eq!(model.entities().len(), 2);
        assert_eq!(snapshot.entities().len(), 1);
    }
}
