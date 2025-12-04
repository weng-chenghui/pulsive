//! Entity models for database storage.

use native_db::*;
use native_model::{native_model, Model};
use pulsive_core::{DefId, EntityId, ValueMap};
use serde::{Deserialize, Serialize};

/// Stored entity in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[native_model(id = 1, version = 1)]
#[native_db]
pub struct StoredEntity {
    /// Primary key - entity ID.
    #[primary_key]
    pub id: u64,
    /// Entity type (kind).
    #[secondary_key]
    pub kind: String,
    /// Serialized properties.
    pub properties: Vec<u8>,
    /// Active flags.
    pub flags: Vec<String>,
}

impl StoredEntity {
    /// Create from a pulsive Entity.
    pub fn from_entity(entity: &pulsive_core::Entity) -> Self {
        let properties = bincode::serialize(&entity.properties).unwrap_or_default();
        Self {
            id: entity.id.raw(),
            kind: entity.kind.as_str().to_string(),
            properties,
            flags: entity
                .flags
                .iter()
                .map(|f| f.as_str().to_string())
                .collect(),
        }
    }

    /// Convert to a pulsive Entity.
    pub fn to_entity(&self) -> pulsive_core::Entity {
        let properties: ValueMap = bincode::deserialize(&self.properties).unwrap_or_default();
        let mut entity =
            pulsive_core::Entity::new(EntityId::new(self.id), DefId::new(self.kind.clone()));
        entity.properties = properties;
        entity.flags = self.flags.iter().map(|f| DefId::new(f.clone())).collect();
        entity
    }
}

/// Stored global state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[native_model(id = 2, version = 1)]
#[native_db]
pub struct StoredGlobals {
    /// Always "globals" - single row.
    #[primary_key]
    pub id: String,
    /// Serialized global variables.
    pub data: Vec<u8>,
}

impl StoredGlobals {
    /// Create from a ValueMap.
    pub fn from_globals(globals: &ValueMap) -> Self {
        let data = bincode::serialize(globals).unwrap_or_default();
        Self {
            id: "globals".to_string(),
            data,
        }
    }

    /// Convert to a ValueMap.
    pub fn to_globals(&self) -> ValueMap {
        bincode::deserialize(&self.data).unwrap_or_default()
    }
}

/// Stored clock state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[native_model(id = 3, version = 1)]
#[native_db]
pub struct StoredClock {
    /// Always "time" - single row.
    #[primary_key]
    pub id: String,
    /// Current tick.
    pub tick: u64,
    /// Processing speed (serialized).
    pub speed: u8,
    /// Ticks per day.
    pub ticks_per_day: u32,
    /// Start year.
    pub start_year: i32,
    /// Start month.
    pub start_month: u8,
    /// Start day.
    pub start_day: u8,
}

impl StoredClock {
    /// Create from Clock.
    pub fn from_clock(clock: &pulsive_core::Clock) -> Self {
        use pulsive_core::Speed;
        let speed = match clock.speed {
            Speed::Paused => 0,
            Speed::VerySlow => 1,
            Speed::Slow => 2,
            Speed::Normal => 3,
            Speed::Fast => 4,
            Speed::VeryFast => 5,
        };
        Self {
            id: "time".to_string(),
            tick: clock.tick,
            speed,
            ticks_per_day: clock.ticks_per_day,
            start_year: clock.start_date.year,
            start_month: clock.start_date.month,
            start_day: clock.start_date.day,
        }
    }

    /// Convert to Clock.
    pub fn to_clock(&self) -> pulsive_core::Clock {
        use pulsive_core::time::Timestamp;
        use pulsive_core::{Clock, Speed};
        let speed = match self.speed {
            0 => Speed::Paused,
            1 => Speed::VerySlow,
            2 => Speed::Slow,
            3 => Speed::Normal,
            4 => Speed::Fast,
            _ => Speed::VeryFast,
        };
        Clock {
            tick: self.tick,
            speed,
            ticks_per_day: self.ticks_per_day,
            start_date: Timestamp::new(self.start_year, self.start_month, self.start_day),
        }
    }
}

/// Stored RNG state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[native_model(id = 4, version = 1)]
#[native_db]
pub struct StoredRng {
    /// Always "rng" - single row.
    #[primary_key]
    pub id: String,
    /// RNG state.
    pub state: u64,
}

impl StoredRng {
    /// Create from GameRng.
    pub fn from_rng(rng: &pulsive_core::GameRng) -> Self {
        Self {
            id: "rng".to_string(),
            state: rng.state(),
        }
    }

    /// Convert to GameRng.
    pub fn to_rng(&self) -> pulsive_core::GameRng {
        pulsive_core::GameRng::from_state(self.state)
    }
}
