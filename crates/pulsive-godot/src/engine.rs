//! Main engine class for Godot integration

use godot::prelude::*;
use pulsive_core::{ActorId, DefId, EntityRef, Model, Msg, Runtime, Speed, UpdateResult};
use pulsive_db::Store;
use pulsive_script::{GameDefs, Loader};
use std::path::PathBuf;

use crate::bridge::{dict_to_value_map, value_map_to_dict, value_to_variant, variant_to_value};

/// The main Pulsive engine exposed to Godot
#[derive(GodotClass)]
#[class(base=Node)]
pub struct PulsiveEngine {
    base: Base<Node>,
    /// The model (state)
    model: Model,
    /// The runtime (event processing)
    runtime: Runtime,
    /// Database store (optional)
    store: Option<Store>,
    /// Definitions loaded from scripts
    defs: GameDefs,
    /// Path to the database file
    db_path: GString,
    /// Path to the scripts directory
    scripts_path: GString,
}

#[godot_api]
impl INode for PulsiveEngine {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            model: Model::new(),
            runtime: Runtime::new(),
            store: None,
            defs: GameDefs::new(),
            db_path: GString::new(),
            scripts_path: GString::new(),
        }
    }

    fn ready(&mut self) {
        godot_print!("Pulsive Engine initialized");
    }
}

#[godot_api]
impl PulsiveEngine {
    // === Configuration ===

    /// Set the path to the database file
    #[func]
    fn set_db_path(&mut self, path: GString) {
        self.db_path = path;
    }

    /// Set the path to the scripts directory
    #[func]
    fn set_scripts_path(&mut self, path: GString) {
        self.scripts_path = path;
    }

    // === Initialization ===

    /// Initialize the engine with the configured paths
    #[func]
    fn initialize(&mut self) -> bool {
        // Load scripts if path is set
        if !self.scripts_path.is_empty() {
            let path = PathBuf::from(self.scripts_path.to_string());
            if path.exists() {
                let mut loader = Loader::new();
                if let Err(e) = loader.load_directory(&path) {
                    godot_error!("Failed to load scripts: {}", e);
                    return false;
                }
                self.defs = loader.finish();
                godot_print!(
                    "Loaded {} resources, {} events, {} entity types",
                    self.defs.resources.len(),
                    self.defs.events.len(),
                    self.defs.entity_types.len()
                );
            }
        }

        // Open database if path is set
        if !self.db_path.is_empty() {
            let path = PathBuf::from(self.db_path.to_string());
            match Store::open(&path) {
                Ok(store) => {
                    self.store = Some(store);
                    godot_print!("Database opened at {:?}", path);
                }
                Err(e) => {
                    godot_error!("Failed to open database: {}", e);
                    return false;
                }
            }
        }

        true
    }

    /// Initialize with an in-memory database (for testing)
    #[func]
    fn initialize_in_memory(&mut self) -> bool {
        match Store::in_memory() {
            Ok(store) => {
                self.store = Some(store);
                godot_print!("In-memory database created");
                true
            }
            Err(e) => {
                godot_error!("Failed to create in-memory database: {}", e);
                false
            }
        }
    }

    // === Model/State Access ===

    /// Create a new entity of the given type
    #[func]
    fn create_entity(&mut self, kind: GString) -> i64 {
        let entity = self.model.entities.create(kind.to_string());
        entity.id.raw() as i64
    }

    /// Get an entity's property
    #[func]
    fn get_property(&self, entity_id: i64, property: GString) -> Variant {
        let id = pulsive_core::EntityId::new(entity_id as u64);
        if let Some(entity) = self.model.entities.get(id) {
            if let Some(value) = entity.get(&property.to_string()) {
                return value_to_variant(value);
            }
        }
        Variant::nil()
    }

    /// Set an entity's property
    #[func]
    fn set_property(&mut self, entity_id: i64, property: GString, value: Variant) {
        let id = pulsive_core::EntityId::new(entity_id as u64);
        if let Some(entity) = self.model.entities.get_mut(id) {
            entity.set(property.to_string(), variant_to_value(&value));
        }
    }

    /// Get all properties of an entity as a VarDictionary
    #[func]
    fn get_entity(&self, entity_id: i64) -> VarDictionary {
        let id = pulsive_core::EntityId::new(entity_id as u64);
        if let Some(entity) = self.model.entities.get(id) {
            return value_map_to_dict(&entity.properties);
        }
        VarDictionary::new()
    }

    /// Delete an entity
    #[func]
    fn delete_entity(&mut self, entity_id: i64) -> bool {
        let id = pulsive_core::EntityId::new(entity_id as u64);
        self.model.entities.remove(id).is_some()
    }

    /// Get all entity IDs of a given type
    #[func]
    fn entities_by_kind(&self, kind: GString) -> PackedInt64Array {
        let def_id = DefId::new(kind.to_string());
        let ids: Vec<i64> = self
            .model
            .entities
            .by_kind(&def_id)
            .map(|e| e.id.raw() as i64)
            .collect();
        PackedInt64Array::from(ids.as_slice())
    }

    // === Global State ===

    /// Get a global property
    #[func]
    fn get_global(&self, property: GString) -> Variant {
        if let Some(value) = self.model.get_global(&property.to_string()) {
            return value_to_variant(value);
        }
        Variant::nil()
    }

    /// Set a global property
    #[func]
    fn set_global(&mut self, property: GString, value: Variant) {
        self.model
            .set_global(property.to_string(), variant_to_value(&value));
    }

    // === Time Control ===

    /// Get the current tick
    #[func]
    fn get_tick(&self) -> i64 {
        self.model.current_tick() as i64
    }

    /// Get the current date as a string
    #[func]
    fn get_date_string(&self) -> GString {
        GString::from(self.model.time.current_date().to_string().as_str())
    }

    /// Set the processing speed
    #[func]
    fn set_speed(&mut self, speed: i32) {
        let sim_speed = match speed {
            0 => Speed::Paused,
            1 => Speed::VerySlow,
            2 => Speed::Slow,
            3 => Speed::Normal,
            4 => Speed::Fast,
            5 => Speed::VeryFast,
            _ => Speed::Normal,
        };
        self.model.time.set_speed(sim_speed);
    }

    /// Get the current processing speed
    #[func]
    fn get_speed(&self) -> i32 {
        match self.model.time.speed {
            Speed::Paused => 0,
            Speed::VerySlow => 1,
            Speed::Slow => 2,
            Speed::Normal => 3,
            Speed::Fast => 4,
            Speed::VeryFast => 5,
        }
    }

    /// Check if the system is paused
    #[func]
    fn is_paused(&self) -> bool {
        self.model.time.speed.is_paused()
    }

    /// Toggle pause
    #[func]
    fn toggle_pause(&mut self) {
        let prev_speed = self.model.time.speed;
        self.model.time.toggle_pause(prev_speed);
    }

    // === Simulation ===

    /// Advance the simulation by one tick
    #[func]
    fn tick(&mut self) -> VarDictionary {
        let result = self.runtime.tick(&mut self.model);
        self.update_result_to_dict(&result)
    }

    /// Send an actor command
    #[func]
    fn send_action(
        &mut self,
        action_type: GString,
        target_id: i64,
        params: VarDictionary,
    ) -> VarDictionary {
        let target = if target_id >= 0 {
            EntityRef::Entity(pulsive_core::EntityId::new(target_id as u64))
        } else {
            EntityRef::Global
        };

        let msg = Msg::command(
            action_type.to_string(),
            target,
            ActorId::new(1), // Default actor
            self.model.current_tick(),
        );

        // Add params
        let mut msg = msg;
        msg.params = dict_to_value_map(&params);

        self.runtime.send(msg);
        let result = self.runtime.process_queue(&mut self.model);
        self.update_result_to_dict(&result)
    }

    /// Send an event
    #[func]
    fn emit_event(
        &mut self,
        event_id: GString,
        target_id: i64,
        params: VarDictionary,
    ) -> VarDictionary {
        let target = if target_id >= 0 {
            EntityRef::Entity(pulsive_core::EntityId::new(target_id as u64))
        } else {
            EntityRef::Global
        };

        let msg = Msg::event(event_id.to_string(), target, self.model.current_tick());

        let mut msg = msg;
        msg.params = dict_to_value_map(&params);

        self.runtime.send(msg);
        let result = self.runtime.process_queue(&mut self.model);
        self.update_result_to_dict(&result)
    }

    // === Persistence ===

    /// Save the current state to the database
    #[func]
    fn save(&mut self) -> bool {
        if let Some(ref store) = self.store {
            if let Err(e) = store.save_model(&self.model) {
                godot_error!("Failed to save: {}", e);
                return false;
            }
            return true;
        }
        godot_error!("No database configured");
        false
    }

    /// Load state from the database
    #[func]
    fn load(&mut self) -> bool {
        if let Some(ref store) = self.store {
            match store.load_model() {
                Ok(model) => {
                    self.model = model;
                    return true;
                }
                Err(e) => {
                    godot_error!("Failed to load: {}", e);
                    return false;
                }
            }
        }
        godot_error!("No database configured");
        false
    }

    // === Helpers ===

    fn update_result_to_dict(&self, result: &UpdateResult) -> VarDictionary {
        let mut dict = VarDictionary::new();

        // Spawned entities
        let spawned: Vec<i64> = result
            .effect_result
            .spawned
            .iter()
            .map(|id| id.raw() as i64)
            .collect();
        dict.set("spawned", PackedInt64Array::from(spawned.as_slice()));

        // Destroyed entities
        let destroyed: Vec<i64> = result
            .effect_result
            .destroyed
            .iter()
            .map(|id| id.raw() as i64)
            .collect();
        dict.set("destroyed", PackedInt64Array::from(destroyed.as_slice()));

        // Logs
        let mut logs = Array::new();
        for (level, message) in &result.effect_result.logs {
            let mut log_dict = VarDictionary::new();
            log_dict.set("level", format!("{:?}", level).to_variant());
            log_dict.set("message", message.to_variant());
            logs.push(&log_dict.to_variant());
        }
        dict.set("logs", logs);

        // Notifications
        let mut notifications = Array::new();
        for notification in &result.effect_result.notifications {
            let mut notif_dict = VarDictionary::new();
            notif_dict.set("kind", notification.kind.as_str().to_variant());
            notif_dict.set("title", notification.title.to_variant());
            notif_dict.set("message", notification.message.to_variant());
            notifications.push(&notif_dict.to_variant());
        }
        dict.set("notifications", notifications);

        dict
    }
}
