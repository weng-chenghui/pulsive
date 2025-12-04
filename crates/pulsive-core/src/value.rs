//! Dynamic value types for data-driven content

use crate::identity::EntityId;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A dynamic value that can represent any game data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum Value {
    /// No value / null
    #[default]
    Null,
    /// Boolean value
    Bool(bool),
    /// Integer value (for counts, IDs, etc.)
    Int(i64),
    /// Floating point value (for resources, percentages, etc.)
    Float(f64),
    /// String value
    String(String),
    /// Reference to an entity
    EntityRef(EntityId),
    /// List of values
    List(Vec<Value>),
    /// Map of string keys to values
    Map(ValueMap),
}

/// A map of string keys to dynamic values
///
/// Uses IndexMap to preserve insertion order (useful for deterministic serialization)
pub type ValueMap = IndexMap<String, Value>;

impl Value {
    /// Check if this value is null
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// Try to get this value as a boolean
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Try to get this value as an integer
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// Try to get this value as a float
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            Value::Int(i) => Some(*i as f64),
            _ => None,
        }
    }

    /// Try to get this value as a string
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// Try to get this value as an entity reference
    pub fn as_entity_ref(&self) -> Option<EntityId> {
        match self {
            Value::EntityRef(id) => Some(*id),
            _ => None,
        }
    }

    /// Try to get this value as a list
    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Value::List(list) => Some(list),
            _ => None,
        }
    }

    /// Try to get this value as a map
    pub fn as_map(&self) -> Option<&ValueMap> {
        match self {
            Value::Map(map) => Some(map),
            _ => None,
        }
    }

    /// Get the type name of this value
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::String(_) => "string",
            Value::EntityRef(_) => "entity_ref",
            Value::List(_) => "list",
            Value::Map(_) => "map",
        }
    }

    /// Check if this value is truthy
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::EntityRef(_) => true,
            Value::List(list) => !list.is_empty(),
            Value::Map(map) => !map.is_empty(),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => write!(f, "null"),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Int(i) => write!(f, "{}", i),
            Value::Float(fl) => write!(f, "{}", fl),
            Value::String(s) => write!(f, "\"{}\"", s),
            Value::EntityRef(id) => write!(f, "{}", id),
            Value::List(list) => {
                write!(f, "[")?;
                for (i, v) in list.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", v)?;
                }
                write!(f, "]")
            }
            Value::Map(map) => {
                write!(f, "{{")?;
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, "}}")
            }
        }
    }
}

// Convenient From implementations
impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl From<i64> for Value {
    fn from(i: i64) -> Self {
        Value::Int(i)
    }
}

impl From<i32> for Value {
    fn from(i: i32) -> Self {
        Value::Int(i as i64)
    }
}

impl From<f64> for Value {
    fn from(f: f64) -> Self {
        Value::Float(f)
    }
}

impl From<f32> for Value {
    fn from(f: f32) -> Self {
        Value::Float(f as f64)
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(s)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(s.to_string())
    }
}

impl From<EntityId> for Value {
    fn from(id: EntityId) -> Self {
        Value::EntityRef(id)
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(vec: Vec<T>) -> Self {
        Value::List(vec.into_iter().map(Into::into).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_types() {
        assert!(Value::Null.is_null());
        assert_eq!(Value::Bool(true).as_bool(), Some(true));
        assert_eq!(Value::Int(42).as_int(), Some(42));
        assert_eq!(Value::Float(3.14).as_float(), Some(3.14));
        assert_eq!(Value::Int(42).as_float(), Some(42.0));
        assert_eq!(Value::String("hello".into()).as_str(), Some("hello"));
    }

    #[test]
    fn test_value_truthiness() {
        assert!(!Value::Null.is_truthy());
        assert!(!Value::Bool(false).is_truthy());
        assert!(Value::Bool(true).is_truthy());
        assert!(!Value::Int(0).is_truthy());
        assert!(Value::Int(1).is_truthy());
        assert!(!Value::String("".into()).is_truthy());
        assert!(Value::String("hello".into()).is_truthy());
    }

    #[test]
    fn test_value_from() {
        let _: Value = true.into();
        let _: Value = 42i64.into();
        let _: Value = 3.14f64.into();
        let _: Value = "hello".into();
        let _: Value = vec![1i64, 2, 3].into();
    }
}
