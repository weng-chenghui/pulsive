//! Pulsive Netcode - Network synchronization patterns
//!
//! This crate provides netcode patterns for multiplayer and distributed systems:
//!
//! - **Prediction**: Apply inputs locally before server confirmation
//! - **Reconciliation**: Correct local state when server state differs
//! - **Interpolation**: Smooth rendering between discrete states
//! - **Input Buffering**: Queue and manage pending commands
//! - **Authority**: Client/server state ownership
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                      Client                                 │
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐  │
//! │  │ Input Buffer │─▶│  Prediction  │─▶│  Interpolation   │  │
//! │  └──────────────┘  └──────────────┘  └──────────────────┘  │
//! │         │                  ▲                   │            │
//! │         ▼                  │                   ▼            │
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐  │
//! │  │   Network    │  │Reconciliation│  │     Render       │  │
//! │  └──────────────┘  └──────────────┘  └──────────────────┘  │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use pulsive_core::{Model, Runtime, StateHistory};
//! use pulsive_netcode::{PredictionEngine, Interpolator};
//! use pulsive_rollback_buffer::RollbackBuffer;
//!
//! // Create prediction engine with rollback buffer
//! let buffer = RollbackBuffer::new(128);
//! let mut prediction = PredictionEngine::new(buffer);
//!
//! // Client loop
//! loop {
//!     // Collect input and predict locally
//!     let input = get_player_input();
//!     prediction.predict(&mut model, &mut runtime, input, current_tick);
//!     
//!     // When server state arrives, reconcile
//!     if let Some(server_state) = receive_server_state() {
//!         prediction.reconcile(&mut model, &mut runtime, server_state);
//!     }
//!     
//!     // Render with interpolation
//!     let render_state = interpolator.interpolate(&model, render_tick);
//!     render(&render_state);
//! }
//! ```

mod error;
mod input_buffer;
mod interpolation;
mod prediction;
mod reconciliation;
mod transport;

pub use error::{Error, Result};
pub use input_buffer::{InputBuffer, InputEntry};
pub use interpolation::Interpolator;
pub use prediction::PredictionEngine;
pub use reconciliation::Reconciler;
pub use transport::{Address, Connection, Transport};

// Re-export core trait for convenience
pub use pulsive_core::StateHistory;
