//! System model (state)

use crate::{ActorId, Clock, Context, EntityStore, GameRng, ValueMap};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// The complete system state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    /// All entities in the system
    pub entities: EntityStore,
    /// Global properties
    pub globals: ValueMap,
    /// System clock
    pub time: Clock,
    /// Deterministic RNG
    pub rng: GameRng,
    /// Actor contexts
    pub actors: IndexMap<ActorId, Context>,
}

impl Model {
    /// Create a new empty model
    pub fn new() -> Self {
        Self {
            entities: EntityStore::new(),
            globals: ValueMap::new(),
            time: Clock::new(),
            rng: GameRng::new(12345),
            actors: IndexMap::new(),
        }
    }

    /// Create with a specific RNG seed
    pub fn with_seed(seed: u64) -> Self {
        Self {
            entities: EntityStore::new(),
            globals: ValueMap::new(),
            time: Clock::new(),
            rng: GameRng::new(seed),
            actors: IndexMap::new(),
        }
    }

    /// Get a global property
    pub fn get_global(&self, key: &str) -> Option<&crate::Value> {
        self.globals.get(key)
    }

    /// Set a global property
    pub fn set_global(&mut self, key: impl Into<String>, value: impl Into<crate::Value>) {
        self.globals.insert(key.into(), value.into());
    }

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

    /// Advance the clock by one tick
    pub fn advance_tick(&mut self) {
        self.time.advance();
    }

    /// Get the current tick
    pub fn current_tick(&self) -> u64 {
        self.time.tick
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
    use crate::Value;

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
}
