//! RON script loader

use crate::error::{Error, Result};
use crate::schema::{EntityTypeDef, EventDef, ResourceDef};
use pulsive_core::DefId;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Loaded game definitions
#[derive(Debug, Default)]
pub struct GameDefs {
    /// Resource definitions by ID
    pub resources: HashMap<DefId, ResourceDef>,
    /// Event definitions by ID
    pub events: HashMap<DefId, EventDef>,
    /// Entity type definitions by ID
    pub entity_types: HashMap<DefId, EntityTypeDef>,
}

impl GameDefs {
    /// Create empty game definitions
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a resource definition
    pub fn get_resource(&self, id: &DefId) -> Option<&ResourceDef> {
        self.resources.get(id)
    }

    /// Get an event definition
    pub fn get_event(&self, id: &DefId) -> Option<&EventDef> {
        self.events.get(id)
    }

    /// Get an entity type definition
    pub fn get_entity_type(&self, id: &DefId) -> Option<&EntityTypeDef> {
        self.entity_types.get(id)
    }
}

/// Loader for RON game scripts
pub struct Loader {
    defs: GameDefs,
}

impl Loader {
    /// Create a new loader
    pub fn new() -> Self {
        Self {
            defs: GameDefs::new(),
        }
    }

    /// Load a single RON file
    pub fn load_file(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)?;
        
        // Try to determine the type based on content or filename
        let filename = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        if filename.contains("resource") || content.contains("resources:") {
            self.load_resources_str(&content)?;
        } else if filename.contains("event") || content.contains("events:") {
            self.load_events_str(&content)?;
        } else if filename.contains("entity") || content.contains("entity_types:") {
            self.load_entity_types_str(&content)?;
        } else {
            // Try each format
            if let Ok(()) = self.load_resources_str(&content) {
                return Ok(());
            }
            if let Ok(()) = self.load_events_str(&content) {
                return Ok(());
            }
            if let Ok(()) = self.load_entity_types_str(&content) {
                return Ok(());
            }
            
            // Try as single definitions
            self.load_single_definition(&content)?;
        }

        Ok(())
    }

    /// Load resources from a RON string
    pub fn load_resources_str(&mut self, content: &str) -> Result<()> {
        #[derive(serde::Deserialize)]
        struct ResourceFile {
            resources: Vec<ResourceDef>,
        }

        let file: ResourceFile = ron::from_str(content)?;
        for resource in file.resources {
            let id = resource.id.clone();
            if self.defs.resources.contains_key(&id) {
                return Err(Error::DuplicateDefinition(id.to_string()));
            }
            self.defs.resources.insert(id, resource);
        }
        Ok(())
    }

    /// Load events from a RON string
    pub fn load_events_str(&mut self, content: &str) -> Result<()> {
        #[derive(serde::Deserialize)]
        struct EventFile {
            events: Vec<EventDef>,
        }

        let file: EventFile = ron::from_str(content)?;
        for event in file.events {
            let id = event.id.clone();
            if self.defs.events.contains_key(&id) {
                return Err(Error::DuplicateDefinition(id.to_string()));
            }
            self.defs.events.insert(id, event);
        }
        Ok(())
    }

    /// Load entity types from a RON string
    pub fn load_entity_types_str(&mut self, content: &str) -> Result<()> {
        #[derive(serde::Deserialize)]
        struct EntityTypeFile {
            entity_types: Vec<EntityTypeDef>,
        }

        let file: EntityTypeFile = ron::from_str(content)?;
        for entity_type in file.entity_types {
            let id = entity_type.id.clone();
            if self.defs.entity_types.contains_key(&id) {
                return Err(Error::DuplicateDefinition(id.to_string()));
            }
            self.defs.entity_types.insert(id, entity_type);
        }
        Ok(())
    }

    /// Try to load a single definition
    fn load_single_definition(&mut self, content: &str) -> Result<()> {
        // Try as single resource
        if let Ok(resource) = ron::from_str::<ResourceDef>(content) {
            let id = resource.id.clone();
            if self.defs.resources.contains_key(&id) {
                return Err(Error::DuplicateDefinition(id.to_string()));
            }
            self.defs.resources.insert(id, resource);
            return Ok(());
        }

        // Try as single event
        if let Ok(event) = ron::from_str::<EventDef>(content) {
            let id = event.id.clone();
            if self.defs.events.contains_key(&id) {
                return Err(Error::DuplicateDefinition(id.to_string()));
            }
            self.defs.events.insert(id, event);
            return Ok(());
        }

        // Try as single entity type
        if let Ok(entity_type) = ron::from_str::<EntityTypeDef>(content) {
            let id = entity_type.id.clone();
            if self.defs.entity_types.contains_key(&id) {
                return Err(Error::DuplicateDefinition(id.to_string()));
            }
            self.defs.entity_types.insert(id, entity_type);
            return Ok(());
        }

        Err(Error::InvalidSchema("Could not parse as any known definition type".to_string()))
    }

    /// Load all RON files from a directory
    pub fn load_directory(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        
        if !path.is_dir() {
            return Err(Error::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Not a directory: {:?}", path),
            )));
        }

        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let file_path = entry.path();
            
            if file_path.extension().map(|e| e == "ron").unwrap_or(false) {
                self.load_file(&file_path)?;
            } else if file_path.is_dir() {
                // Recursively load subdirectories
                self.load_directory(&file_path)?;
            }
        }

        Ok(())
    }

    /// Finish loading and return the game definitions
    pub fn finish(self) -> GameDefs {
        self.defs
    }

    /// Get the current definitions (for inspection during loading)
    pub fn defs(&self) -> &GameDefs {
        &self.defs
    }
}

impl Default for Loader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_resources() {
        let content = r#"
        (
            resources: [
                (
                    id: "gold",
                    name: "Gold",
                    base_value: 1.0,
                    tradeable: true,
                ),
                (
                    id: "manpower",
                    name: "Manpower",
                    base_value: 0.5,
                ),
            ]
        )
        "#;

        let mut loader = Loader::new();
        loader.load_resources_str(content).unwrap();
        
        let defs = loader.finish();
        assert!(defs.get_resource(&DefId::new("gold")).is_some());
        assert!(defs.get_resource(&DefId::new("manpower")).is_some());
    }

    #[test]
    fn test_load_single_resource() {
        let content = r#"
        (
            id: "gold",
            name: "Gold",
            base_value: 1.0,
            tradeable: true,
        )
        "#;

        let mut loader = Loader::new();
        loader.load_single_definition(content).unwrap();
        
        let defs = loader.finish();
        assert!(defs.get_resource(&DefId::new("gold")).is_some());
    }
}
