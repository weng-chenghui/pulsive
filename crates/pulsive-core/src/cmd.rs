//! Commands (side effects) produced by the update function

use crate::{DefId, EntityRef, Msg, ValueMap};
use serde::{Deserialize, Serialize};

/// A command to be executed by the runtime
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Cmd {
    /// No operation
    None,

    /// Batch multiple commands
    Batch(Vec<Cmd>),

    /// Emit a message (will be processed in the next update cycle)
    Emit(Msg),

    /// Schedule a message for a future tick
    Schedule {
        msg: Msg,
        delay_ticks: u64,
    },

    /// Persist the current state to the database
    PersistState,

    /// Load state from the database
    LoadState,

    /// Send a notification to the UI (Godot)
    Notify {
        kind: DefId,
        title: String,
        message: String,
        target: EntityRef,
        params: ValueMap,
    },

    /// Play a sound effect
    PlaySound {
        sound_id: DefId,
        volume: f32,
    },

    /// Request to save the game
    SaveGame {
        slot: String,
    },

    /// Request to load a saved game
    LoadGame {
        slot: String,
    },

    /// Log a message for debugging
    Log {
        level: LogLevel,
        message: String,
    },
}

/// Log level for debug commands
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl Cmd {
    /// Create an empty command
    pub fn none() -> Self {
        Cmd::None
    }

    /// Create a batch of commands
    pub fn batch(cmds: Vec<Cmd>) -> Self {
        // Flatten nested batches and filter out None
        let flattened: Vec<Cmd> = cmds
            .into_iter()
            .flat_map(|cmd| match cmd {
                Cmd::None => vec![],
                Cmd::Batch(inner) => inner,
                other => vec![other],
            })
            .collect();

        if flattened.is_empty() {
            Cmd::None
        } else if flattened.len() == 1 {
            flattened.into_iter().next().unwrap()
        } else {
            Cmd::Batch(flattened)
        }
    }

    /// Create an emit command
    pub fn emit(msg: Msg) -> Self {
        Cmd::Emit(msg)
    }

    /// Create a schedule command
    pub fn schedule(msg: Msg, delay_ticks: u64) -> Self {
        Cmd::Schedule { msg, delay_ticks }
    }

    /// Create a notification command
    pub fn notify(kind: impl Into<DefId>, title: impl Into<String>, message: impl Into<String>) -> Self {
        Cmd::Notify {
            kind: kind.into(),
            title: title.into(),
            message: message.into(),
            target: EntityRef::None,
            params: ValueMap::new(),
        }
    }

    /// Create a log command
    pub fn log(level: LogLevel, message: impl Into<String>) -> Self {
        Cmd::Log {
            level,
            message: message.into(),
        }
    }

    /// Create a debug log command
    pub fn debug(message: impl Into<String>) -> Self {
        Self::log(LogLevel::Debug, message)
    }

    /// Create an info log command
    pub fn info(message: impl Into<String>) -> Self {
        Self::log(LogLevel::Info, message)
    }

    /// Check if this is a None command
    pub fn is_none(&self) -> bool {
        matches!(self, Cmd::None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cmd_batch() {
        let cmd = Cmd::batch(vec![
            Cmd::None,
            Cmd::debug("hello"),
            Cmd::None,
        ]);
        
        // Should flatten to single command
        matches!(cmd, Cmd::Log { .. });
    }

    #[test]
    fn test_cmd_batch_nested() {
        let cmd = Cmd::batch(vec![
            Cmd::batch(vec![Cmd::debug("a"), Cmd::debug("b")]),
            Cmd::debug("c"),
        ]);
        
        if let Cmd::Batch(cmds) = cmd {
            assert_eq!(cmds.len(), 3);
        } else {
            panic!("Expected Batch");
        }
    }
}

