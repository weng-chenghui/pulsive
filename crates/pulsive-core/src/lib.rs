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

mod value;
mod identity;
mod expr;
pub mod effect;
pub mod time;
mod rng;
mod entity;
mod model;
mod msg;
mod cmd;
mod actor;
pub mod runtime;
mod error;

pub use value::{Value, ValueMap};
pub use identity::{EntityId, DefId};
pub use expr::{Expr, EvalContext};
pub use effect::{Effect, ModifyOp, EffectResult};
pub use time::{Clock, Tick, Speed, Timestamp};
pub use rng::GameRng;
pub use entity::{Entity, EntityStore, EntityRef};
pub use model::Model;
pub use msg::{Msg, MsgKind};
pub use cmd::Cmd;
pub use actor::{ActorId, Command, Context};
pub use runtime::{Runtime, UpdateResult, EventHandler, TickHandler};
pub use error::{Error, Result};
