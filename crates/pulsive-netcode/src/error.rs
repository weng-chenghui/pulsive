//! Error types for pulsive-netcode

use thiserror::Error;

/// Netcode error type
#[derive(Debug, Error)]
pub enum Error {
    /// State not found for rollback
    #[error("State not found for tick {0}")]
    StateNotFound(u64),

    /// Rollback too far in the past
    #[error("Cannot rollback to tick {target}, oldest available is {oldest}")]
    RollbackTooFar { target: u64, oldest: u64 },

    /// Input buffer overflow
    #[error("Input buffer full, cannot queue more inputs")]
    InputBufferFull,

    /// Prediction failed
    #[error("Prediction failed: {0}")]
    PredictionFailed(String),

    /// Reconciliation failed
    #[error("Reconciliation failed: {0}")]
    ReconciliationFailed(String),

    /// Transport error
    #[error("Transport error: {0}")]
    Transport(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Result type for netcode operations
pub type Result<T> = std::result::Result<T, Error>;
