//! Pulsive Journal - Auditing, replay, and time-travel debugging
//!
//! This crate builds on `pulsive-core`'s journal infrastructure to provide:
//!
//! - **Auditor**: Query and analyze recorded events for compliance and analytics
//! - **Replayer**: Replay sessions with fine-grained control
//! - **Exporter**: Export journal data to various formats
//!
//! # Example
//!
//! ```rust,ignore
//! use pulsive_core::{Model, Runtime, Journal};
//! use pulsive_journal::{Auditor, Replayer, Exporter};
//!
//! // Record a session
//! let mut model = Model::new();
//! let mut runtime = Runtime::new();
//! let mut journal = Journal::new();
//! journal.start_recording();
//!
//! for _ in 0..100 {
//!     runtime.tick_with_journal(&mut model, &mut journal);
//! }
//!
//! // Audit the session
//! let auditor = Auditor::new(&journal);
//! let report = auditor.generate_report();
//! println!("{}", report);
//!
//! // Replay to a specific point
//! let mut replayer = Replayer::new(&journal);
//! replayer.goto(&mut model, &mut runtime, 50);
//!
//! // Export for external analysis
//! let exporter = Exporter::new(&journal);
//! let json = exporter.to_json()?;
//! ```

mod auditor;
mod error;
mod exporter;
mod replayer;

pub use auditor::{AuditQuery, AuditReport, Auditor, EventSummary};
pub use error::{Error, Result};
pub use exporter::{ExportFormat, Exporter};
pub use replayer::{ReplaySpeed, ReplayState, Replayer};

// Re-export core journal types for convenience
pub use pulsive_core::{Journal, JournalConfig, JournalEntry, JournalStats, Snapshot, SnapshotId};
