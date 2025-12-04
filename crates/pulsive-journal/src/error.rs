//! Error types for pulsive-journal

use thiserror::Error;

/// Journal error type
#[derive(Debug, Error)]
pub enum Error {
    /// Invalid tick range
    #[error("Invalid tick range: {0}..{1}")]
    InvalidTickRange(u64, u64),

    /// Snapshot not found
    #[error("Snapshot not found: {0}")]
    SnapshotNotFound(u64),

    /// Replay error
    #[error("Replay error: {0}")]
    ReplayError(String),

    /// Export error
    #[error("Export error: {0}")]
    ExportError(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Result type for journal operations
pub type Result<T> = std::result::Result<T, Error>;
