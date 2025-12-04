//! Pulsive DB - Database layer using native_db
//!
//! Provides persistent storage for:
//! - Entity definitions (schemas loaded from scripts)
//! - Runtime entity instances
//! - Event definitions and triggers

mod error;
mod models;
mod queries;
mod store;

pub use error::{Error, Result};
pub use store::Store;
