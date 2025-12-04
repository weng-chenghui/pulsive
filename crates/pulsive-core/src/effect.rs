//! Effect types for modifying game state
//!
//! Effects are the "write" side of the expression engine.
//! They describe changes to be made to entities and game state.

use crate::{DefId, EntityRef, Expr, ValueMap};
use serde::{Deserialize, Serialize};

/// An operation to modify a numeric value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModifyOp {
    /// Set to the value
    Set,
    /// Add the value
    Add,
    /// Subtract the value
    Sub,
    /// Multiply by the value
    Mul,
    /// Divide by the value
    Div,
    /// Set to minimum of current and value
    Min,
    /// Set to maximum of current and value
    Max,
}

impl ModifyOp {
    /// Apply this operation to a current value
    pub fn apply(&self, current: f64, operand: f64) -> f64 {
        match self {
            ModifyOp::Set => operand,
            ModifyOp::Add => current + operand,
            ModifyOp::Sub => current - operand,
            ModifyOp::Mul => current * operand,
            ModifyOp::Div => {
                if operand != 0.0 {
                    current / operand
                } else {
                    current
                }
            }
            ModifyOp::Min => current.min(operand),
            ModifyOp::Max => current.max(operand),
        }
    }
}

/// An effect that modifies game state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Effect {
    // === Property Modification ===
    /// Set a property on the target entity
    SetProperty {
        property: String,
        value: Expr,
    },
    /// Modify a numeric property on the target entity
    ModifyProperty {
        property: String,
        op: ModifyOp,
        value: Expr,
    },
    /// Set a property on a specific entity
    SetEntityProperty {
        target: EntityRef,
        property: String,
        value: Expr,
    },
    /// Modify a numeric property on a specific entity
    ModifyEntityProperty {
        target: EntityRef,
        property: String,
        op: ModifyOp,
        value: Expr,
    },
    /// Set a global property
    SetGlobal {
        property: String,
        value: Expr,
    },
    /// Modify a global numeric property
    ModifyGlobal {
        property: String,
        op: ModifyOp,
        value: Expr,
    },

    // === Flags ===
    /// Add a flag to the target entity
    AddFlag(DefId),
    /// Remove a flag from the target entity
    RemoveFlag(DefId),
    /// Add a flag to a specific entity
    AddEntityFlag {
        target: EntityRef,
        flag: DefId,
    },
    /// Remove a flag from a specific entity
    RemoveEntityFlag {
        target: EntityRef,
        flag: DefId,
    },

    // === Entity Lifecycle ===
    /// Spawn a new entity
    SpawnEntity {
        kind: DefId,
        properties: Vec<(String, Expr)>,
    },
    /// Destroy the target entity
    DestroyTarget,
    /// Destroy a specific entity
    DestroyEntity(EntityRef),

    // === Events ===
    /// Emit an event (triggers event handlers)
    EmitEvent {
        event: DefId,
        target: EntityRef,
        params: Vec<(String, Expr)>,
    },
    /// Schedule an event for a future tick
    ScheduleEvent {
        event: DefId,
        target: EntityRef,
        delay_ticks: Expr,
        params: Vec<(String, Expr)>,
    },

    // === Control Flow ===
    /// Execute effects conditionally
    If {
        condition: Expr,
        then_effects: Vec<Effect>,
        else_effects: Vec<Effect>,
    },
    /// Execute multiple effects
    Sequence(Vec<Effect>),
    /// Execute effects for each entity of a kind
    ForEachEntity {
        kind: DefId,
        filter: Option<Expr>,
        effects: Vec<Effect>,
    },
    /// Choose one branch randomly based on weights
    RandomChoice {
        choices: Vec<(Expr, Vec<Effect>)>, // (weight, effects)
    },

    // === Output ===
    /// Log a message (for debugging)
    Log {
        level: LogLevel,
        message: Expr,
    },
    /// Send a notification to the UI
    Notify {
        kind: DefId,
        title: Expr,
        message: Expr,
        target: EntityRef,
    },
}

/// Log level for debug output
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl Effect {
    /// Create a set property effect
    pub fn set(property: impl Into<String>, value: Expr) -> Self {
        Effect::SetProperty {
            property: property.into(),
            value,
        }
    }

    /// Create an add effect (adds to a numeric property)
    pub fn add(property: impl Into<String>, value: Expr) -> Self {
        Effect::ModifyProperty {
            property: property.into(),
            op: ModifyOp::Add,
            value,
        }
    }

    /// Create a multiply effect
    pub fn multiply(property: impl Into<String>, value: Expr) -> Self {
        Effect::ModifyProperty {
            property: property.into(),
            op: ModifyOp::Mul,
            value,
        }
    }

    /// Create an add flag effect
    pub fn flag(flag: impl Into<DefId>) -> Self {
        Effect::AddFlag(flag.into())
    }

    /// Create a spawn entity effect
    pub fn spawn(kind: impl Into<DefId>) -> Self {
        Effect::SpawnEntity {
            kind: kind.into(),
            properties: Vec::new(),
        }
    }

    /// Create an emit event effect
    pub fn emit(event: impl Into<DefId>, target: EntityRef) -> Self {
        Effect::EmitEvent {
            event: event.into(),
            target,
            params: Vec::new(),
        }
    }

    /// Create a sequence of effects
    pub fn seq(effects: Vec<Effect>) -> Self {
        Effect::Sequence(effects)
    }

    /// Create a conditional effect
    pub fn when(condition: Expr, then_effects: Vec<Effect>) -> Self {
        Effect::If {
            condition,
            then_effects,
            else_effects: Vec::new(),
        }
    }
}

/// Result of executing an effect
#[derive(Debug, Clone, Default)]
pub struct EffectResult {
    /// Entities that were spawned
    pub spawned: Vec<crate::EntityId>,
    /// Entities that were destroyed
    pub destroyed: Vec<crate::EntityId>,
    /// Events that were emitted
    pub emitted_events: Vec<(DefId, EntityRef, ValueMap)>,
    /// Scheduled events (event, target, delay, params)
    pub scheduled_events: Vec<(DefId, EntityRef, u64, ValueMap)>,
    /// Log messages
    pub logs: Vec<(LogLevel, String)>,
    /// Notifications
    pub notifications: Vec<Notification>,
}

/// A notification to send to the UI
#[derive(Debug, Clone)]
pub struct Notification {
    pub kind: DefId,
    pub title: String,
    pub message: String,
    pub target: EntityRef,
}

impl EffectResult {
    /// Create an empty result
    pub fn new() -> Self {
        Self::default()
    }

    /// Merge another result into this one
    pub fn merge(&mut self, other: EffectResult) {
        self.spawned.extend(other.spawned);
        self.destroyed.extend(other.destroyed);
        self.emitted_events.extend(other.emitted_events);
        self.scheduled_events.extend(other.scheduled_events);
        self.logs.extend(other.logs);
        self.notifications.extend(other.notifications);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modify_op() {
        assert_eq!(ModifyOp::Set.apply(10.0, 5.0), 5.0);
        assert_eq!(ModifyOp::Add.apply(10.0, 5.0), 15.0);
        assert_eq!(ModifyOp::Sub.apply(10.0, 5.0), 5.0);
        assert_eq!(ModifyOp::Mul.apply(10.0, 5.0), 50.0);
        assert_eq!(ModifyOp::Div.apply(10.0, 5.0), 2.0);
        assert_eq!(ModifyOp::Min.apply(10.0, 5.0), 5.0);
        assert_eq!(ModifyOp::Max.apply(10.0, 5.0), 10.0);
    }

    #[test]
    fn test_effect_builders() {
        let effect = Effect::set("gold", Expr::lit(100.0));
        matches!(effect, Effect::SetProperty { .. });

        let effect = Effect::add("gold", Expr::lit(50.0));
        matches!(effect, Effect::ModifyProperty { op: ModifyOp::Add, .. });

        let effect = Effect::flag("at_war");
        matches!(effect, Effect::AddFlag(_));
    }
}

