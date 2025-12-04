//! Entity types for game objects

use crate::{DefId, EntityId, Value, ValueMap};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Reference to an entity or a special target
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum EntityRef {
    /// No target
    #[default]
    None,
    /// Reference to a specific entity by ID
    Entity(EntityId),
    /// Reference to a global/system target
    Global,
    /// Reference by definition ID (e.g., "nation:france")
    ByDef(DefId),
}

impl EntityRef {
    /// Check if this is a none reference
    pub fn is_none(&self) -> bool {
        matches!(self, EntityRef::None)
    }

    /// Try to get the entity ID if this is a direct reference
    pub fn as_entity_id(&self) -> Option<EntityId> {
        match self {
            EntityRef::Entity(id) => Some(*id),
            _ => None,
        }
    }
}

impl From<EntityId> for EntityRef {
    fn from(id: EntityId) -> Self {
        EntityRef::Entity(id)
    }
}

/// A dynamic entity instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    /// Unique identifier for this entity
    pub id: EntityId,
    /// The type of this entity (references an entity type definition)
    pub kind: DefId,
    /// Dynamic properties (e.g., {"gold": 100.0, "stability": 2})
    pub properties: ValueMap,
    /// Active flags/modifiers on this entity
    pub flags: HashSet<DefId>,
}

impl Entity {
    /// Create a new entity
    pub fn new(id: EntityId, kind: impl Into<DefId>) -> Self {
        Self {
            id,
            kind: kind.into(),
            properties: ValueMap::new(),
            flags: HashSet::new(),
        }
    }

    /// Get a property value
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.properties.get(key)
    }

    /// Get a property value or a default
    pub fn get_or(&self, key: &str, default: Value) -> Value {
        self.properties.get(key).cloned().unwrap_or(default)
    }

    /// Set a property value
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<Value>) {
        self.properties.insert(key.into(), value.into());
    }

    /// Remove a property
    pub fn remove(&mut self, key: &str) -> Option<Value> {
        self.properties.shift_remove(key)
    }

    /// Check if entity has a flag
    pub fn has_flag(&self, flag: &DefId) -> bool {
        self.flags.contains(flag)
    }

    /// Add a flag
    pub fn add_flag(&mut self, flag: impl Into<DefId>) {
        self.flags.insert(flag.into());
    }

    /// Remove a flag
    pub fn remove_flag(&mut self, flag: &DefId) -> bool {
        self.flags.remove(flag)
    }

    /// Get a numeric property as f64
    pub fn get_number(&self, key: &str) -> Option<f64> {
        self.properties.get(key).and_then(|v| v.as_float())
    }

    /// Modify a numeric property
    pub fn modify_number(&mut self, key: &str, delta: f64) {
        let current = self.get_number(key).unwrap_or(0.0);
        self.set(key, current + delta);
    }
}

/// Storage for all entities in the game
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntityStore {
    /// All entities by ID
    entities: IndexMap<EntityId, Entity>,
    /// Next entity ID to assign
    next_id: u64,
    /// Index: kind -> entity IDs
    by_kind: IndexMap<DefId, Vec<EntityId>>,
}

impl EntityStore {
    /// Create a new empty entity store
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new entity and add it to the store
    pub fn create(&mut self, kind: impl Into<DefId>) -> &mut Entity {
        let id = EntityId::new(self.next_id);
        self.next_id += 1;
        let kind = kind.into();

        // Add to kind index
        self.by_kind.entry(kind.clone()).or_default().push(id);

        // Create and store entity
        let entity = Entity::new(id, kind);
        self.entities.insert(id, entity);
        self.entities.get_mut(&id).unwrap()
    }

    /// Get an entity by ID
    pub fn get(&self, id: EntityId) -> Option<&Entity> {
        self.entities.get(&id)
    }

    /// Get a mutable reference to an entity
    pub fn get_mut(&mut self, id: EntityId) -> Option<&mut Entity> {
        self.entities.get_mut(&id)
    }

    /// Remove an entity
    pub fn remove(&mut self, id: EntityId) -> Option<Entity> {
        if let Some(entity) = self.entities.shift_remove(&id) {
            // Remove from kind index
            if let Some(ids) = self.by_kind.get_mut(&entity.kind) {
                ids.retain(|&eid| eid != id);
            }
            Some(entity)
        } else {
            None
        }
    }

    /// Get all entities of a given kind
    pub fn by_kind(&self, kind: &DefId) -> impl Iterator<Item = &Entity> {
        self.by_kind
            .get(kind)
            .into_iter()
            .flat_map(|ids| ids.iter().filter_map(|id| self.entities.get(id)))
    }

    /// Get all entity IDs
    pub fn ids(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.entities.keys().copied()
    }

    /// Get all entities
    pub fn iter(&self) -> impl Iterator<Item = &Entity> {
        self.entities.values()
    }

    /// Get all entities mutably
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Entity> {
        self.entities.values_mut()
    }

    /// Get the number of entities
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// Check if the store is empty
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// Resolve an EntityRef to an Entity
    pub fn resolve(&self, entity_ref: &EntityRef) -> Option<&Entity> {
        match entity_ref {
            EntityRef::None => None,
            EntityRef::Entity(id) => self.get(*id),
            EntityRef::Global => None, // Global has no entity
            EntityRef::ByDef(def) => self.by_kind(def).next(),
        }
    }

    /// Resolve an EntityRef to a mutable Entity
    pub fn resolve_mut(&mut self, entity_ref: &EntityRef) -> Option<&mut Entity> {
        match entity_ref {
            EntityRef::None => None,
            EntityRef::Entity(id) => self.get_mut(*id),
            EntityRef::Global => None,
            EntityRef::ByDef(def) => {
                // Need to get ID first to avoid borrow issues
                let id = self.by_kind.get(def).and_then(|ids| ids.first()).copied();
                id.and_then(move |id| self.get_mut(id))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity() {
        let mut entity = Entity::new(EntityId::new(1), "nation");
        entity.set("gold", 100.0f64);
        entity.set("name", "France");
        entity.add_flag(DefId::new("at_war"));

        assert_eq!(entity.get_number("gold"), Some(100.0));
        assert_eq!(entity.get("name").and_then(|v| v.as_str()), Some("France"));
        assert!(entity.has_flag(&DefId::new("at_war")));
    }

    #[test]
    fn test_entity_store() {
        let mut store = EntityStore::new();

        let entity = store.create("nation");
        entity.set("name", "France");
        let france_id = entity.id;

        let entity = store.create("nation");
        entity.set("name", "England");
        let england_id = entity.id;

        let entity = store.create("province");
        entity.set("name", "Paris");

        assert_eq!(store.len(), 3);
        assert_eq!(store.by_kind(&DefId::new("nation")).count(), 2);
        assert_eq!(store.by_kind(&DefId::new("province")).count(), 1);

        assert!(store.get(france_id).is_some());
        assert!(store.get(england_id).is_some());
    }
}
