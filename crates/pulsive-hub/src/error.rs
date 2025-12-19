//! Error types for pulsive-hub

use crate::conflict::ConflictReport;
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

    /// Unresolved conflicts during WriteSets merge
    ///
    /// This error is returned when conflicts are detected and the resolution
    /// strategy is `Abort`, or when conflicts cannot be automatically resolved.
    #[error("unresolved conflicts: {0} conflict(s) detected")]
    UnresolvedConflicts(ConflictReport),

    /// Core error
    #[error("core error: {0}")]
    Core(#[from] pulsive_core::Error),
}
