//! Pulsive Script - RON loader and schema definitions
//!
//! Loads game content from RON files:
//! - Resource definitions
//! - Event definitions with conditions and effects
//! - Entity type schemas

mod error;
mod loader;
mod schema;

pub use error::{Error, Result};
pub use loader::{GameDefs, Loader};
pub use schema::entity::{EntityTypeDefs, PropertyDef, PropertyType};
pub use schema::event::{EventDefs, EventOption, MeanTimeToHappen, MtthModifier};
pub use schema::resource::ResourceDefs;
pub use schema::{EntityTypeDef, EventDef, ResourceDef};
