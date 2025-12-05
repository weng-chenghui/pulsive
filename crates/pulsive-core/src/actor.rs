//! Actor types for multi-actor reactive systems
//!
//! Actors represent any principal that can submit commands to the system:
//! - Users in web applications
//! - Participants in simulations
//! - Services in microservices
//! - Automated processes or bots

use crate::{DefId, EntityId, EntityRef, Value, ValueMap};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Unique identifier for an actor
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ActorId(pub u64);

impl ActorId {
    /// Create a new actor ID
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// The system actor (for automated/background actions)
    pub const SYSTEM: ActorId = ActorId(0);

    /// Get the raw ID value
    pub fn raw(&self) -> u64 {
        self.0
    }

    /// Check if this is the system actor
    pub fn is_system(&self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for ActorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_system() {
            write!(f, "actor:system")
        } else {
            write!(f, "actor:{}", self.0)
        }
    }
}

/// A command submitted by an actor for processing
///
/// Commands are validated actions that modify system state.
/// In single-actor mode: validated locally
/// In multi-actor mode: sent to coordinator for validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    /// Which actor submitted this command
    pub actor_id: ActorId,
    /// The type of command (references a command definition)
    pub action: DefId,
    /// Target entity for the command
    pub target: EntityRef,
    /// Command parameters
    pub params: ValueMap,
    /// Which tick this command targets
    pub tick: u64,
}

impl Command {
    /// Create a new command
    pub fn new(actor_id: ActorId, action: impl Into<DefId>, target: EntityRef) -> Self {
        Self {
            actor_id,
            action: action.into(),
            target,
            params: ValueMap::new(),
            tick: 0,
        }
    }

    /// Add a parameter to the command
    pub fn with_param(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }

    /// Set the target tick
    pub fn at_tick(mut self, tick: u64) -> Self {
        self.tick = tick;
        self
    }
}

/// Context about an actor's session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    /// The actor's ID
    pub id: ActorId,
    /// Entities this actor controls
    pub controlled_entities: Vec<EntityId>,
    /// Whether this actor is connected (for distributed systems)
    pub connected: bool,
    /// Whether this actor is ready to advance
    pub ready: bool,
}

impl Context {
    /// Create a new actor context
    pub fn new(id: ActorId) -> Self {
        Self {
            id,
            controlled_entities: Vec::new(),
            connected: true,
            ready: false,
        }
    }

    /// Add an entity to this actor's control
    pub fn add_controlled_entity(&mut self, entity: EntityId) {
        if !self.controlled_entities.contains(&entity) {
            self.controlled_entities.push(entity);
        }
    }

    /// Check if this actor controls an entity
    pub fn controls(&self, entity: EntityId) -> bool {
        self.controlled_entities.contains(&entity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_actor_id() {
        let system = ActorId::SYSTEM;
        assert!(system.is_system());
        assert_eq!(format!("{}", system), "actor:system");

        let actor = ActorId::new(1);
        assert!(!actor.is_system());
        assert_eq!(format!("{}", actor), "actor:1");
    }

    #[test]
    fn test_command() {
        let cmd = Command::new(
            ActorId::new(1),
            "build_unit",
            EntityRef::Entity(EntityId::new(100)),
        )
        .with_param("unit_type", "infantry")
        .with_param("count", 5i64)
        .at_tick(42);

        assert_eq!(cmd.actor_id, ActorId::new(1));
        assert_eq!(cmd.action.as_str(), "build_unit");
        assert_eq!(cmd.tick, 42);
    }

    #[test]
    fn test_context() {
        let mut ctx = Context::new(ActorId::new(1));
        let entity = EntityId::new(100);

        assert!(!ctx.controls(entity));
        ctx.add_controlled_entity(entity);
        assert!(ctx.controls(entity));
    }
}
