//! Pulsive Hub - MVCC Orchestrator for Parallel Execution
//!
//! This crate provides the coordination layer for running multiple pulsive-core
//! instances in parallel with MVCC-style snapshot isolation.
//!
//! ## Architecture
//!
//! ```text
//! Hub (owns global model)
//!  │
//!  ├── CoreGroup (trait) ← Hub only interacts with this
//!  │    │
//!  │    └── Core[] ← Hidden from Hub
//!  │         └── Runtime + Local Model
//!  │
//!  └── Global Model + Journal
//! ```
//!
//! ## Key Components
//!
//! - [`Hub`]: Central coordinator that owns the global model
//! - [`CoreGroup`]: Trait for groups of cores with different execution strategies
//! - [`TickSyncGroup`]: Implementation where all cores stay at the same tick
//! - [`Core`]: Thin wrapper bundling pulsive-core's Runtime + Model
//!
//! ## Design Principles
//!
//! 1. **Hub never touches Cores directly** - only through CoreGroup trait
//! 2. **pulsive-core is standalone** - it does NOT know about pulsive-hub
//! 3. **Core is just a wrapper** - bundles Runtime+Model, delegates all logic to pulsive-core

pub mod commit;
pub mod conflict;
mod core;
mod error;
mod group;
mod hub;
mod snapshot;
mod tick_sync;

pub use commit::{apply, apply_batch};
pub use conflict::{
    default_conflict_filter, detect_conflicts, detect_conflicts_filtered, resolve_conflicts,
    Conflict, ConflictReport, ConflictResolver, ConflictTarget, ConflictType, ResolutionResult,
    ResolutionStrategy, ResolvedConflict,
};
pub use core::{Core, CoreId};
pub use error::{Error, Result};
pub use group::{CoreGroup, GroupId};
pub use hub::Hub;
pub use snapshot::ModelSnapshot;
pub use tick_sync::TickSyncGroup;
