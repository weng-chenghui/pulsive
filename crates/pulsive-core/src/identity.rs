//! Identity types for entities and definitions

use serde::{Deserialize, Serialize};
use std::fmt;

/// Unique identifier for an entity instance at runtime
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId(pub u64);

impl EntityId {
    /// Create a new entity ID
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// Get the raw ID value
    pub fn raw(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "entity:{}", self.0)
    }
}

/// Identifier for a definition (type, event, resource, etc.) loaded from scripts
/// 
/// Uses a string-based ID for easy reference from RON scripts
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DefId(pub String);

impl DefId {
    /// Create a new definition ID
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Get the ID as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DefId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for DefId {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for DefId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_id() {
        let id = EntityId::new(42);
        assert_eq!(id.raw(), 42);
        assert_eq!(format!("{}", id), "entity:42");
    }

    #[test]
    fn test_def_id() {
        let id = DefId::new("gold");
        assert_eq!(id.as_str(), "gold");
        assert_eq!(format!("{}", id), "gold");
    }
}

