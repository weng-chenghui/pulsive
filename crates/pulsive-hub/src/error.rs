//! Error types for pulsive-hub

use thiserror::Error;

/// Result type for pulsive-hub operations
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur in pulsive-hub
#[derive(Debug, Error)]
pub enum Error {
    /// No groups registered in the hub
    #[error("no core groups registered in hub")]
    NoGroups,

    /// Group not found
    #[error("group {0:?} not found")]
    GroupNotFound(crate::GroupId),

    /// Core error
    #[error("core error: {0}")]
    Core(#[from] pulsive_core::Error),
}
