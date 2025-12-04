//! Message types for the reactive system

use crate::{DefId, EntityRef, ActorId, ValueMap};
use serde::{Deserialize, Serialize};

/// The kind of message
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MsgKind {
    /// System tick (advance time)
    Tick,
    /// Actor command (validated)
    Command,
    /// Event triggered by conditions
    Event,
    /// Scheduled event firing
    ScheduledEvent,
    /// Entity spawned
    EntitySpawned,
    /// Entity destroyed
    EntityDestroyed,
    /// Property changed
    PropertyChanged,
    /// Flag added
    FlagAdded,
    /// Flag removed
    FlagRemoved,
    /// Custom message type (defined in scripts)
    Custom(DefId),
}

/// A message in the reactive system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Msg {
    /// The kind of message
    pub kind: MsgKind,
    /// Event or action ID (if applicable)
    pub event_id: Option<DefId>,
    /// Target entity (if applicable)
    pub target: EntityRef,
    /// Message parameters
    pub params: ValueMap,
    /// Which actor triggered this (if applicable)
    pub actor: Option<ActorId>,
    /// The tick when this message was created
    pub tick: u64,
}

impl Msg {
    /// Create a new message
    pub fn new(kind: MsgKind) -> Self {
        Self {
            kind,
            event_id: None,
            target: EntityRef::None,
            params: ValueMap::new(),
            actor: None,
            tick: 0,
        }
    }

    /// Create a tick message
    pub fn tick(tick: u64) -> Self {
        Self {
            kind: MsgKind::Tick,
            event_id: None,
            target: EntityRef::None,
            params: ValueMap::new(),
            actor: None,
            tick,
        }
    }

    /// Create an event message
    pub fn event(event_id: impl Into<DefId>, target: EntityRef, tick: u64) -> Self {
        Self {
            kind: MsgKind::Event,
            event_id: Some(event_id.into()),
            target,
            params: ValueMap::new(),
            actor: None,
            tick,
        }
    }

    /// Create a command message
    pub fn command(
        action_id: impl Into<DefId>,
        target: EntityRef,
        actor: ActorId,
        tick: u64,
    ) -> Self {
        Self {
            kind: MsgKind::Command,
            event_id: Some(action_id.into()),
            target,
            params: ValueMap::new(),
            actor: Some(actor),
            tick,
        }
    }

    /// Set the event ID
    pub fn with_event(mut self, event_id: impl Into<DefId>) -> Self {
        self.event_id = Some(event_id.into());
        self
    }

    /// Set the target
    pub fn with_target(mut self, target: EntityRef) -> Self {
        self.target = target;
        self
    }

    /// Add a parameter
    pub fn with_param(mut self, key: impl Into<String>, value: impl Into<crate::Value>) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }

    /// Set the actor
    pub fn with_actor(mut self, actor: ActorId) -> Self {
        self.actor = Some(actor);
        self
    }

    /// Set the tick
    pub fn at_tick(mut self, tick: u64) -> Self {
        self.tick = tick;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_msg_tick() {
        let msg = Msg::tick(42);
        assert_eq!(msg.kind, MsgKind::Tick);
        assert_eq!(msg.tick, 42);
    }

    #[test]
    fn test_msg_event() {
        let msg = Msg::event("peasant_uprising", EntityRef::Global, 10)
            .with_param("severity", 5i64);
        
        assert_eq!(msg.kind, MsgKind::Event);
        assert_eq!(msg.event_id, Some(DefId::new("peasant_uprising")));
        assert!(msg.params.contains_key("severity"));
    }
}
