//! Error types for pulsive-script

use thiserror::Error;

/// Script loading error type
#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("RON parse error: {0}")]
    Ron(#[from] ron::error::SpannedError),

    #[error("Invalid schema: {0}")]
    InvalidSchema(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Duplicate definition: {0}")]
    DuplicateDefinition(String),
}

/// Result type alias
pub type Result<T> = std::result::Result<T, Error>;
