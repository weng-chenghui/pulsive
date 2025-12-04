//! Resource definition schema

use pulsive_core::DefId;
use serde::{Deserialize, Serialize};

/// Definition of a resource type (e.g., gold, manpower, grain)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceDef {
    /// Unique identifier for this resource
    pub id: DefId,
    /// Display name
    pub name: String,
    /// Description
    #[serde(default)]
    pub description: String,
    /// Base value for trading/conversion
    #[serde(default = "default_base_value")]
    pub base_value: f64,
    /// Whether this resource can be traded
    #[serde(default)]
    pub tradeable: bool,
    /// Decay rate per tick (0.0 = no decay)
    #[serde(default)]
    pub decay_rate: f64,
    /// Minimum value (floor)
    #[serde(default)]
    pub min_value: Option<f64>,
    /// Maximum value (cap)
    #[serde(default)]
    pub max_value: Option<f64>,
    /// Icon identifier for UI
    #[serde(default)]
    pub icon: Option<String>,
    /// Color for UI (hex string)
    #[serde(default)]
    pub color: Option<String>,
}

fn default_base_value() -> f64 {
    1.0
}

impl ResourceDef {
    /// Create a new resource definition
    pub fn new(id: impl Into<DefId>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            base_value: 1.0,
            tradeable: false,
            decay_rate: 0.0,
            min_value: None,
            max_value: None,
            icon: None,
            color: None,
        }
    }
}

/// A collection of resource definitions
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceDefs {
    pub resources: Vec<ResourceDef>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_def_ron() {
        let ron_str = r#"
        (
            id: "gold",
            name: "Gold",
            description: "The primary currency",
            base_value: 1.0,
            tradeable: true,
            decay_rate: 0.0,
            min_value: Some(0.0),
        )
        "#;

        let def: ResourceDef = ron::from_str(ron_str).unwrap();
        assert_eq!(def.id.as_str(), "gold");
        assert_eq!(def.name, "Gold");
        assert!(def.tradeable);
    }
}
