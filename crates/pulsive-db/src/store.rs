//! Database store wrapper.

use crate::error::{Error, Result};
use crate::models::*;
use native_db::*;
use pulsive_core::{Clock, Entity, EntityId, Model, Rng, ValueMap};
use std::path::Path;
use std::sync::LazyLock;

// Static models for the database
static MODELS: LazyLock<Models> = LazyLock::new(|| {
    let mut models = Models::new();
    models.define::<StoredEntity>().unwrap();
    models.define::<StoredGlobals>().unwrap();
    models.define::<StoredClock>().unwrap();
    models.define::<StoredRng>().unwrap();
    models.define::<StoredResourceDef>().unwrap();
    models.define::<StoredEntityTypeDef>().unwrap();
    models.define::<StoredEventDef>().unwrap();
    models.define::<StoredScheduledEvent>().unwrap();
    models
});

/// Database store for persistent game state.
pub struct Store {
    pub(crate) db: Database<'static>,
}

impl Store {
    /// Open or create a database at the given path.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let db = Builder::new()
            .create(&MODELS, path.as_ref())
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(Self { db })
    }

    /// Create an in-memory database.
    pub fn in_memory() -> Result<Self> {
        let db = Builder::new()
            .create_in_memory(&MODELS)
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(Self { db })
    }

    /// Save an entity.
    pub fn save_entity(&self, entity: &Entity) -> Result<()> {
        let stored = StoredEntity::from_entity(entity);
        let rw = self.db.rw_transaction()?;
        rw.upsert(stored)?;
        rw.commit()?;
        Ok(())
    }

    /// Load an entity by ID.
    pub fn load_entity(&self, id: EntityId) -> Result<Option<Entity>> {
        let r = self.db.r_transaction()?;
        let stored: Option<StoredEntity> = r.get().primary(id.raw())?;
        Ok(stored.map(|s| s.to_entity()))
    }

    /// Delete an entity.
    pub fn delete_entity(&self, id: EntityId) -> Result<()> {
        let rw = self.db.rw_transaction()?;
        let stored: Option<StoredEntity> = rw.get().primary(id.raw())?;
        if let Some(s) = stored {
            rw.remove(s)?;
        }
        rw.commit()?;
        Ok(())
    }

    /// Load all entities.
    pub fn load_all_entities(&self) -> Result<Vec<Entity>> {
        let r = self.db.r_transaction()?;
        let scan = r.scan().primary::<StoredEntity>()?;
        let iter = scan.all()?;
        let entities: std::result::Result<Vec<StoredEntity>, _> = iter.collect();
        let entities = entities.map_err(|e| Error::Database(e.to_string()))?;
        Ok(entities.into_iter().map(|e| e.to_entity()).collect())
    }

    /// Save global variables.
    pub fn save_globals(&self, globals: &ValueMap) -> Result<()> {
        let stored = StoredGlobals::from_globals(globals);
        let rw = self.db.rw_transaction()?;
        rw.upsert(stored)?;
        rw.commit()?;
        Ok(())
    }

    /// Load global variables.
    pub fn load_globals(&self) -> Result<ValueMap> {
        let r = self.db.r_transaction()?;
        let stored: Option<StoredGlobals> = r.get().primary("globals".to_string())?;
        Ok(stored.map(|s| s.to_globals()).unwrap_or_default())
    }

    /// Save game time.
    pub fn save_clock(&self, clock: &Clock) -> Result<()> {
        let stored = StoredClock::from_clock(clock);
        let rw = self.db.rw_transaction()?;
        rw.upsert(stored)?;
        rw.commit()?;
        Ok(())
    }

    /// Load game time.
    pub fn load_clock(&self) -> Result<Option<Clock>> {
        let r = self.db.r_transaction()?;
        let stored: Option<StoredClock> = r.get().primary("time".to_string())?;
        Ok(stored.map(|s| s.to_clock()))
    }

    /// Save RNG state.
    pub fn save_rng(&self, rng: &Rng) -> Result<()> {
        let stored = StoredRng::from_rng(rng);
        let rw = self.db.rw_transaction()?;
        rw.upsert(stored)?;
        rw.commit()?;
        Ok(())
    }

    /// Load RNG state.
    pub fn load_rng(&self) -> Result<Option<Rng>> {
        let r = self.db.r_transaction()?;
        let stored: Option<StoredRng> = r.get().primary("rng".to_string())?;
        Ok(stored.map(|s| s.to_rng()))
    }

    /// Save a complete model.
    pub fn save_model(&self, model: &Model) -> Result<()> {
        let rw = self.db.rw_transaction()?;

        // Save all entities
        for entity in model.entities.iter() {
            let stored = StoredEntity::from_entity(entity);
            rw.upsert(stored)?;
        }

        // Save globals
        let globals = StoredGlobals::from_globals(&model.globals);
        rw.upsert(globals)?;

        // Save time
        let time = StoredClock::from_clock(&model.time);
        rw.upsert(time)?;

        // Save RNG
        let rng = StoredRng::from_rng(&model.rng);
        rw.upsert(rng)?;

        rw.commit()?;
        Ok(())
    }

    /// Load a complete model.
    pub fn load_model(&self) -> Result<Model> {
        let mut model = Model::new();

        // Load entities
        for entity in self.load_all_entities()? {
            // Insert entity into the store
            let kind = entity.kind.clone();
            let new_entity = model.entities.create(kind);
            new_entity.properties = entity.properties;
            new_entity.flags = entity.flags;
        }

        // Load globals
        model.globals = self.load_globals()?;

        // Load time
        if let Some(time) = self.load_clock()? {
            model.time = time;
        }

        // Load RNG
        if let Some(rng) = self.load_rng()? {
            model.rng = rng;
        }

        Ok(model)
    }

    /// Clear all data.
    pub fn clear(&self) -> Result<()> {
        // First, collect all entity IDs
        let entity_ids: Vec<u64> = {
            let r = self.db.r_transaction()?;
            let scan = r.scan().primary::<StoredEntity>()?;
            let iter = scan.all()?;
            let entities: std::result::Result<Vec<StoredEntity>, _> = iter.collect();
            let entities = entities.map_err(|e| Error::Database(e.to_string()))?;
            entities.into_iter().map(|e| e.id).collect()
        };

        // Now delete in a separate transaction
        let rw = self.db.rw_transaction()?;

        // Clear entities by ID
        for id in entity_ids {
            if let Some(entity) = rw.get().primary::<StoredEntity>(id)? {
                rw.remove(entity)?;
            }
        }

        // Clear globals
        if let Some(globals) = rw.get().primary::<StoredGlobals>("globals".to_string())? {
            rw.remove(globals)?;
        }

        // Clear time
        if let Some(time) = rw.get().primary::<StoredClock>("time".to_string())? {
            rw.remove(time)?;
        }

        // Clear RNG
        if let Some(rng) = rw.get().primary::<StoredRng>("rng".to_string())? {
            rw.remove(rng)?;
        }

        rw.commit()?;
        Ok(())
    }
}

impl From<native_db::db_type::Error> for Error {
    fn from(err: native_db::db_type::Error) -> Self {
        Error::Database(err.to_string())
    }
}
