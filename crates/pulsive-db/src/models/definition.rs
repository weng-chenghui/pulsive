//! Definition models for database storage.

use native_db::*;
use native_model::{native_model, Model};
use serde::{Deserialize, Serialize};

/// Stored resource definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[native_model(id = 10, version = 1)]
#[native_db]
pub struct StoredResourceDef {
    /// Primary key - resource ID.
    #[primary_key]
    pub id: String,
    /// Display name.
    pub name: String,
    /// Description.
    pub description: Option<String>,
    /// Base value.
    pub base_value: f64,
    /// Whether tradeable.
    pub tradeable: bool,
    /// Decay rate.
    pub decay_rate: f64,
    /// Max storage.
    pub max_storage: Option<f64>,
    /// Category.
    pub category: Option<String>,
}

/// Stored entity type definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[native_model(id = 11, version = 1)]
#[native_db]
pub struct StoredEntityTypeDef {
    /// Primary key - entity type ID.
    #[primary_key]
    pub id: String,
    /// Display name.
    pub name: String,
    /// Description.
    pub description: Option<String>,
    /// Serialized property definitions.
    pub properties: Vec<u8>,
    /// Default flags.
    pub default_flags: Vec<String>,
    /// Default tags.
    pub default_tags: Vec<String>,
    /// Parent entity type.
    pub extends: Option<String>,
}

/// Stored event definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[native_model(id = 12, version = 1)]
#[native_db]
pub struct StoredEventDef {
    /// Primary key - event ID.
    #[primary_key]
    pub id: String,
    /// Display title.
    pub title: String,
    /// Description.
    pub description: String,
    /// Serialized trigger expression.
    pub trigger: Option<Vec<u8>>,
    /// Serialized weight expression.
    pub weight: Option<Vec<u8>>,
    /// Serialized effects.
    pub effects: Vec<u8>,
    /// Serialized options.
    pub options: Vec<u8>,
    /// Whether repeatable.
    pub repeatable: bool,
    /// Tags.
    pub tags: Vec<String>,
}

/// Stored modifier definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[native_model(id = 13, version = 1)]
#[native_db]
pub struct StoredModifierDef {
    /// Primary key - modifier ID.
    #[primary_key]
    pub id: String,
    /// Display name.
    pub name: String,
    /// Description.
    pub description: Option<String>,
    /// Serialized modifiers map.
    pub modifiers: Vec<u8>,
    /// Duration in ticks.
    pub duration: Option<u64>,
    /// Whether stackable.
    pub stackable: bool,
}

/// Stored scheduled event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[native_model(id = 20, version = 1)]
#[native_db]
pub struct StoredScheduledEvent {
    /// Primary key - composite of tick and sequence.
    #[primary_key]
    pub key: String,
    /// Trigger tick.
    #[secondary_key]
    pub trigger_tick: u64,
    /// Event ID.
    pub event_id: String,
    /// Target entity ID or reference.
    pub target: String,
    /// Serialized parameters.
    pub params: Vec<u8>,
}

