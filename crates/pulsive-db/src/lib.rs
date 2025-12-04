//! Pulsive DB - Database layer using native_db
//!
//! Provides persistent storage for:
//! - Entity definitions (schemas loaded from scripts)
//! - Runtime entity instances
//! - Event definitions and triggers

mod store;
mod models;
mod queries;
mod error;

pub use store::Store;
pub use error::{Error, Result};
