//! Error types for database operations.

use thiserror::Error;

/// Errors that can occur during database operations.
#[derive(Debug, Error)]
pub enum Error {
    /// Native DB error.
    #[error("Database error: {0}")]
    Database(String),

    /// Serialization error.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Entity not found.
    #[error("Entity not found: {0}")]
    NotFound(String),

    /// Duplicate key.
    #[error("Duplicate key: {0}")]
    DuplicateKey(String),

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type for database operations.
pub type Result<T> = std::result::Result<T, Error>;
