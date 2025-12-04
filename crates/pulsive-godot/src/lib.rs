//! Pulsive Godot - GDExtension bindings for Godot 4
//!
//! Exposes the pulsive engine to Godot as native classes.

mod bridge;
mod engine;

use godot::prelude::*;

struct PulsiveExtension;

#[gdextension]
unsafe impl ExtensionLibrary for PulsiveExtension {}

// Re-export the main engine class
pub use engine::PulsiveEngine;
