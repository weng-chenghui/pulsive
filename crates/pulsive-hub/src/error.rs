//! Error types for pulsive-hub
//!
//! Note: This module imports `ConflictReport` from `conflict.rs`, while `conflict.rs`
//! uses `crate::Error` and `crate::Result`. This is not a problematic circular dependency
//! in Rust because both modules are in the same crate and the types are only used in
//! function signatures, not in mutually-dependent struct definitions.

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
    ///
    /// The `report` field contains the full conflict details (boxed to avoid
    /// large error sizes during propagation). Use [`Error::conflict_report()`]
    /// for convenient access.
    #[error("unresolved conflicts: {}", Self::format_conflict_count(*.count))]
    UnresolvedConflicts {
        /// Number of conflicts detected
        count: usize,
        /// Full conflict report with details (boxed to reduce error size)
        report: Box<ConflictReport>,
    },

    /// Core error
    #[error("core error: {0}")]
    Core(#[from] pulsive_core::Error),
}

impl Error {
    /// Create an UnresolvedConflicts error from a ConflictReport
    pub fn unresolved_conflicts(report: ConflictReport) -> Self {
        Error::UnresolvedConflicts {
            count: report.len(),
            report: Box::new(report),
        }
    }

    /// Get the conflict report if this is an UnresolvedConflicts error
    pub fn conflict_report(&self) -> Option<&ConflictReport> {
        match self {
            Error::UnresolvedConflicts { report, .. } => Some(report),
            _ => None,
        }
    }

    /// Format conflict count with proper pluralization
    fn format_conflict_count(count: usize) -> String {
        if count == 1 {
            "1 conflict detected".to_string()
        } else {
            format!("{} conflicts detected", count)
        }
    }
}

// Compile-time check that Error is Send + Sync for thread-safe error propagation.
// This function is never called but will fail to compile if the bound is not satisfied.
fn _assert_error_send_sync<T: Send + Sync>() {}
fn _error_is_send_sync() {
    _assert_error_send_sync::<Error>();
}
