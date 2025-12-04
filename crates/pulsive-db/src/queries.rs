//! Common query patterns for the database.

use crate::error::{Error, Result};
use crate::models::*;
use crate::store::Store;
use pulsive_core::Entity;

impl Store {
    /// Get all entities of a specific kind.
    pub fn entities_by_kind(&self, kind: &str) -> Result<Vec<Entity>> {
        let r = self.db.r_transaction()?;
        let scan = r.scan().secondary::<StoredEntity>(StoredEntityKey::kind)?;
        let iter = scan.start_with(kind)?;
        let entities: std::result::Result<Vec<StoredEntity>, _> = iter.collect();
        let entities = entities.map_err(|e| Error::Database(e.to_string()))?;
        Ok(entities.into_iter().map(|e| e.to_entity()).collect())
    }

    /// Count entities of a specific kind.
    pub fn count_entities_by_kind(&self, kind: &str) -> Result<usize> {
        let r = self.db.r_transaction()?;
        let scan = r.scan().secondary::<StoredEntity>(StoredEntityKey::kind)?;
        let iter = scan.start_with(kind)?;
        Ok(iter.count())
    }

    /// Get entities with a specific flag.
    pub fn entities_with_flag(&self, flag: &str) -> Result<Vec<Entity>> {
        let r = self.db.r_transaction()?;
        let scan = r.scan().primary::<StoredEntity>()?;
        let iter = scan.all()?;
        let all: std::result::Result<Vec<StoredEntity>, _> = iter.collect();
        let all = all.map_err(|e| Error::Database(e.to_string()))?;
        Ok(all
            .into_iter()
            .filter(|e| e.flags.contains(&flag.to_string()))
            .map(|e| e.to_entity())
            .collect())
    }

    /// Get scheduled events for a specific tick.
    pub fn scheduled_events_for_tick(&self, tick: u64) -> Result<Vec<StoredScheduledEvent>> {
        let r = self.db.r_transaction()?;
        let scan = r
            .scan()
            .secondary::<StoredScheduledEvent>(StoredScheduledEventKey::trigger_tick)?;
        let iter = scan.start_with(tick)?;
        let events: std::result::Result<Vec<StoredScheduledEvent>, _> = iter.collect();
        events.map_err(|e| Error::Database(e.to_string()))
    }

    /// Get all resource definitions.
    pub fn all_resource_defs(&self) -> Result<Vec<StoredResourceDef>> {
        let r = self.db.r_transaction()?;
        let scan = r.scan().primary::<StoredResourceDef>()?;
        let iter = scan.all()?;
        let defs: std::result::Result<Vec<StoredResourceDef>, _> = iter.collect();
        defs.map_err(|e| Error::Database(e.to_string()))
    }

    /// Get all entity type definitions.
    pub fn all_entity_type_defs(&self) -> Result<Vec<StoredEntityTypeDef>> {
        let r = self.db.r_transaction()?;
        let scan = r.scan().primary::<StoredEntityTypeDef>()?;
        let iter = scan.all()?;
        let defs: std::result::Result<Vec<StoredEntityTypeDef>, _> = iter.collect();
        defs.map_err(|e| Error::Database(e.to_string()))
    }

    /// Get all event definitions.
    pub fn all_event_defs(&self) -> Result<Vec<StoredEventDef>> {
        let r = self.db.r_transaction()?;
        let scan = r.scan().primary::<StoredEventDef>()?;
        let iter = scan.all()?;
        let defs: std::result::Result<Vec<StoredEventDef>, _> = iter.collect();
        defs.map_err(|e| Error::Database(e.to_string()))
    }
}
