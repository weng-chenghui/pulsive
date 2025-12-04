//! Pulsive Script - RON loader and schema definitions
//!
//! Loads game content from RON files:
//! - Resource definitions
//! - Event definitions with conditions and effects
//! - Entity type schemas

mod loader;
mod schema;
mod error;

pub use loader::{Loader, GameDefs};
pub use schema::{ResourceDef, EventDef, EntityTypeDef};
pub use schema::entity::{PropertyDef, PropertyType, EntityTypeDefs};
pub use schema::event::{EventOption, MeanTimeToHappen, MtthModifier, EventDefs};
pub use schema::resource::ResourceDefs;
pub use error::{Error, Result};
