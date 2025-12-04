//! Event definition schema

use pulsive_core::{DefId, Effect, Expr};
use serde::{Deserialize, Serialize};

/// Definition of a game event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDef {
    /// Unique identifier for this event
    pub id: DefId,
    /// Display name
    pub name: String,
    /// Description
    #[serde(default)]
    pub description: String,
    /// Trigger condition (when this event can fire)
    #[serde(default)]
    pub trigger: Option<Expr>,
    /// Mean time to happen (in ticks) - for random events
    #[serde(default)]
    pub mtth: Option<MeanTimeToHappen>,
    /// Weight for random selection among eligible events
    #[serde(default = "default_weight")]
    pub weight: f64,
    /// Whether this event can only fire once per target
    #[serde(default)]
    pub fire_only_once: bool,
    /// Target entity kind (if any)
    #[serde(default)]
    pub target_kind: Option<DefId>,
    /// Immediate effects (before options are shown)
    #[serde(default)]
    pub immediate: Vec<Effect>,
    /// Options the player can choose
    #[serde(default)]
    pub options: Vec<EventOption>,
    /// Category for grouping in UI
    #[serde(default)]
    pub category: Option<DefId>,
    /// Icon for UI
    #[serde(default)]
    pub icon: Option<String>,
}

fn default_weight() -> f64 {
    1.0
}

/// Mean time to happen configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeanTimeToHappen {
    /// Base number of ticks
    pub ticks: u64,
    /// Modifiers that affect the time
    #[serde(default)]
    pub modifiers: Vec<MtthModifier>,
}

/// A modifier for mean time to happen
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MtthModifier {
    /// Condition for this modifier to apply
    pub condition: Expr,
    /// Factor to multiply time by (< 1.0 = more likely, > 1.0 = less likely)
    pub factor: f64,
}

/// An option in an event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventOption {
    /// Unique ID within the event
    pub id: String,
    /// Display text
    pub text: String,
    /// Condition for this option to be available
    #[serde(default)]
    pub condition: Option<Expr>,
    /// Effects when this option is chosen
    #[serde(default)]
    pub effects: Vec<Effect>,
    /// AI weight for choosing this option
    #[serde(default = "default_ai_weight")]
    pub ai_weight: f64,
}

fn default_ai_weight() -> f64 {
    1.0
}

impl EventDef {
    /// Create a new event definition
    pub fn new(id: impl Into<DefId>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            trigger: None,
            mtth: None,
            weight: 1.0,
            fire_only_once: false,
            target_kind: None,
            immediate: Vec::new(),
            options: Vec::new(),
            category: None,
            icon: None,
        }
    }
}

impl EventOption {
    /// Create a new event option
    pub fn new(id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            condition: None,
            effects: Vec::new(),
            ai_weight: 1.0,
        }
    }
}

/// A collection of event definitions
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EventDefs {
    pub events: Vec<EventDef>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_def_basic() {
        let event = EventDef::new("peasant_uprising", "Peasant Uprising");
        assert_eq!(event.id.as_str(), "peasant_uprising");
        assert_eq!(event.weight, 1.0);
    }
}

