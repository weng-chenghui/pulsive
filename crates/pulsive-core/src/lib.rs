//! Pulsive Core - Reactive engine with Elm-style architecture
//!
//! This crate provides the core types and runtime for the pulsive engine:
//! - Dynamic value types (`Value`, `ValueMap`)
//! - Entity and definition identifiers
//! - Expression engine for conditions and effects
//! - Tick-based time and deterministic RNG
//! - Elm-style runtime with Model, Msg, and Cmd
//!
//! ## Generic Reactive Concepts
//!
//! While pulsive was designed for games, its concepts are generic:
//! - `Actor` - Any entity that submits commands (user, service, bot)
//! - `Command` - A validated action to process
//! - `Context` - Session/state for an actor
//! - `Clock` - Simulation time with tick-based progression
//! - `Speed` - Processing rate control
//!
//! ## Journal Feature
//!
//! Enable the `journal` feature for recording, replay, and auditing:
//! ```toml
//! pulsive-core = { version = "0.1", features = ["journal"] }
//! ```

mod actor;
mod cmd;
pub mod effect;
mod entity;
mod error;
mod expr;
mod identity;
mod model;
mod msg;
mod rng;
pub mod runtime;
pub mod time;
mod value;

#[cfg(feature = "journal")]
pub mod journal;

pub use actor::{ActorId, Command, Context};
pub use cmd::Cmd;
pub use effect::{Effect, EffectResult, ModifyOp};
pub use entity::{Entity, EntityRef, EntityStore};
pub use error::{Error, Result};
pub use expr::{EvalContext, Expr};
pub use identity::{DefId, EntityId};
pub use model::Model;
pub use msg::{Msg, MsgKind};
pub use rng::GameRng;
pub use runtime::{EventHandler, Runtime, TickHandler, UpdateResult};
pub use time::{Clock, Speed, Tick, Timestamp};
pub use value::{Value, ValueMap};

#[cfg(feature = "journal")]
pub use journal::{Journal, JournalConfig, JournalEntry, JournalStats, Snapshot, SnapshotId};
