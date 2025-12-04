//! Entity type definition schema

use pulsive_core::{DefId, Value};
use serde::{Deserialize, Serialize};

/// Definition of an entity type (e.g., nation, province, army)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityTypeDef {
    /// Unique identifier for this entity type
    pub id: DefId,
    /// Display name
    pub name: String,
    /// Description
    #[serde(default)]
    pub description: String,
    /// Property schemas for this entity type
    #[serde(default)]
    pub properties: Vec<PropertyDef>,
    /// Default values for properties
    #[serde(default)]
    pub defaults: Vec<(String, Value)>,
    /// Parent entity type (for inheritance)
    #[serde(default)]
    pub extends: Option<DefId>,
    /// Category for grouping
    #[serde(default)]
    pub category: Option<DefId>,
}

/// Definition of a property on an entity type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyDef {
    /// Property name
    pub name: String,
    /// Property type
    pub property_type: PropertyType,
    /// Whether this property is required
    #[serde(default)]
    pub required: bool,
    /// Default value
    #[serde(default)]
    pub default: Option<Value>,
    /// Minimum value (for numeric types)
    #[serde(default)]
    pub min: Option<f64>,
    /// Maximum value (for numeric types)
    #[serde(default)]
    pub max: Option<f64>,
    /// Description
    #[serde(default)]
    pub description: String,
}

/// Property type enum
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PropertyType {
    Bool,
    Int,
    Float,
    String,
    EntityRef,
    DefRef,
    List(Box<PropertyType>),
    Map,
}

impl EntityTypeDef {
    /// Create a new entity type definition
    pub fn new(id: impl Into<DefId>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            properties: Vec::new(),
            defaults: Vec::new(),
            extends: None,
            category: None,
        }
    }

    /// Add a property definition
    pub fn with_property(mut self, prop: PropertyDef) -> Self {
        self.properties.push(prop);
        self
    }
}

impl PropertyDef {
    /// Create a float property
    pub fn float(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            property_type: PropertyType::Float,
            required: false,
            default: None,
            min: None,
            max: None,
            description: String::new(),
        }
    }

    /// Create an int property
    pub fn int(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            property_type: PropertyType::Int,
            required: false,
            default: None,
            min: None,
            max: None,
            description: String::new(),
        }
    }

    /// Create a string property
    pub fn string(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            property_type: PropertyType::String,
            required: false,
            default: None,
            min: None,
            max: None,
            description: String::new(),
        }
    }

    /// Make this property required
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Set a default value
    pub fn with_default(mut self, value: impl Into<Value>) -> Self {
        self.default = Some(value.into());
        self
    }
}

/// A collection of entity type definitions
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EntityTypeDefs {
    pub entity_types: Vec<EntityTypeDef>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_type_def() {
        let nation = EntityTypeDef::new("nation", "Nation")
            .with_property(PropertyDef::string("name").required())
            .with_property(PropertyDef::float("gold").with_default(0.0f64))
            .with_property(PropertyDef::int("stability").with_default(0i64));

        assert_eq!(nation.id.as_str(), "nation");
        assert_eq!(nation.properties.len(), 3);
    }
}
