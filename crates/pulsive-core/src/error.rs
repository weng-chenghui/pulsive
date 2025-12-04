//! Error types for pulsive-core

use thiserror::Error;

/// Core error type
#[derive(Error, Debug)]
pub enum Error {
    #[error("Type error: expected {expected}, got {got}")]
    TypeError { expected: String, got: String },

    #[error("Property not found: {0}")]
    PropertyNotFound(String),

    #[error("Entity not found: {0}")]
    EntityNotFound(String),

    #[error("Definition not found: {0}")]
    DefinitionNotFound(String),

    #[error("Division by zero")]
    DivisionByZero,

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("Evaluation error: {0}")]
    EvaluationError(String),
}

/// Result type alias
pub type Result<T> = std::result::Result<T, Error>;
