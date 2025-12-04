//! Journal infrastructure for recording, replay, and auditing
//!
//! The journal provides:
//! - Message recording for audit trails
//! - State snapshots for efficient replay
//! - Time-travel debugging capabilities
//! - Event sourcing support
//!
//! # Example
//!
//! ```rust,ignore
//! use pulsive_core::{Runtime, Model, Journal};
//!
//! let mut model = Model::new();
//! let mut runtime = Runtime::new();
//! let mut journal = Journal::new();
//!
//! // Enable recording
//! journal.start_recording();
//!
//! // Process messages - they get recorded
//! runtime.tick_with_journal(&mut model, &mut journal);
//!
//! // Replay to a specific tick
//! journal.replay_to(&mut model, 10);
//!
//! // Export for auditing
//! let entries = journal.entries_since(0);
//! ```

use crate::{Model, Msg, Tick};
use serde::{Deserialize, Serialize};

/// A journal entry representing a recorded event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JournalEntry {
    /// A message was processed
    Message {
        /// The tick when this message was processed
        tick: Tick,
        /// The message that was processed
        msg: Msg,
        /// Sequence number within the tick
        seq: u64,
    },
    /// A tick boundary
    TickBoundary {
        /// The tick number
        tick: Tick,
    },
    /// A state snapshot was taken
    Snapshot {
        /// The tick when the snapshot was taken
        tick: Tick,
        /// Unique ID for this snapshot
        snapshot_id: SnapshotId,
    },
    /// Custom metadata entry (for auditing)
    Metadata {
        /// The tick when this was recorded
        tick: Tick,
        /// Key for the metadata
        key: String,
        /// Value (serialized)
        value: String,
    },
}

/// Unique identifier for a snapshot
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SnapshotId(pub u64);

impl SnapshotId {
    /// Create a new snapshot ID
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

/// A stored state snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Unique ID
    pub id: SnapshotId,
    /// The tick when this snapshot was taken
    pub tick: Tick,
    /// The serialized model state
    pub model: Model,
}

/// Configuration for the journal
#[derive(Debug, Clone)]
pub struct JournalConfig {
    /// Whether recording is enabled
    pub recording_enabled: bool,
    /// Take snapshots every N ticks (0 = disabled)
    pub snapshot_interval: u64,
    /// Maximum number of entries to keep (0 = unlimited)
    pub max_entries: usize,
    /// Maximum number of snapshots to keep (0 = unlimited)
    pub max_snapshots: usize,
}

impl Default for JournalConfig {
    fn default() -> Self {
        Self {
            recording_enabled: false,
            snapshot_interval: 100, // Snapshot every 100 ticks by default
            max_entries: 0,         // Unlimited
            max_snapshots: 10,      // Keep last 10 snapshots
        }
    }
}

/// The journal for recording and replaying events
#[derive(Debug, Clone)]
pub struct Journal {
    /// Configuration
    config: JournalConfig,
    /// Recorded entries
    entries: Vec<JournalEntry>,
    /// State snapshots
    snapshots: Vec<Snapshot>,
    /// Current sequence number
    current_seq: u64,
    /// Next snapshot ID
    next_snapshot_id: u64,
    /// Last tick that was recorded
    last_recorded_tick: Option<Tick>,
}

impl Journal {
    /// Create a new journal
    pub fn new() -> Self {
        Self {
            config: JournalConfig::default(),
            entries: Vec::new(),
            snapshots: Vec::new(),
            current_seq: 0,
            next_snapshot_id: 0,
            last_recorded_tick: None,
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: JournalConfig) -> Self {
        Self {
            config,
            entries: Vec::new(),
            snapshots: Vec::new(),
            current_seq: 0,
            next_snapshot_id: 0,
            last_recorded_tick: None,
        }
    }

    /// Start recording
    pub fn start_recording(&mut self) {
        self.config.recording_enabled = true;
    }

    /// Stop recording
    pub fn stop_recording(&mut self) {
        self.config.recording_enabled = false;
    }

    /// Check if recording is enabled
    pub fn is_recording(&self) -> bool {
        self.config.recording_enabled
    }

    /// Record a message being processed
    pub fn record_message(&mut self, tick: Tick, msg: Msg) {
        if !self.config.recording_enabled {
            return;
        }

        // Record tick boundary if this is a new tick
        if self.last_recorded_tick != Some(tick) {
            self.entries.push(JournalEntry::TickBoundary { tick });
            self.last_recorded_tick = Some(tick);
            self.current_seq = 0;
        }

        self.entries.push(JournalEntry::Message {
            tick,
            msg,
            seq: self.current_seq,
        });
        self.current_seq += 1;

        self.enforce_limits();
    }

    /// Record a tick boundary
    pub fn record_tick(&mut self, tick: Tick) {
        if !self.config.recording_enabled {
            return;
        }

        if self.last_recorded_tick != Some(tick) {
            self.entries.push(JournalEntry::TickBoundary { tick });
            self.last_recorded_tick = Some(tick);
            self.current_seq = 0;
        }

        self.enforce_limits();
    }

    /// Take a snapshot of the current model state
    pub fn take_snapshot(&mut self, model: &Model) -> SnapshotId {
        let id = SnapshotId::new(self.next_snapshot_id);
        self.next_snapshot_id += 1;

        let tick = model.current_tick();
        let snapshot = Snapshot {
            id,
            tick,
            model: model.clone(),
        };

        self.snapshots.push(snapshot);

        if self.config.recording_enabled {
            self.entries.push(JournalEntry::Snapshot {
                tick,
                snapshot_id: id,
            });
        }

        self.enforce_snapshot_limits();
        id
    }

    /// Check if a snapshot should be taken at this tick
    pub fn should_snapshot(&self, tick: Tick) -> bool {
        if self.config.snapshot_interval == 0 {
            return false;
        }
        tick.is_multiple_of(self.config.snapshot_interval)
    }

    /// Record custom metadata (for auditing)
    pub fn record_metadata(
        &mut self,
        tick: Tick,
        key: impl Into<String>,
        value: impl Into<String>,
    ) {
        if !self.config.recording_enabled {
            return;
        }

        self.entries.push(JournalEntry::Metadata {
            tick,
            key: key.into(),
            value: value.into(),
        });

        self.enforce_limits();
    }

    /// Get all entries
    pub fn entries(&self) -> &[JournalEntry] {
        &self.entries
    }

    /// Get entries since a specific tick
    pub fn entries_since(&self, tick: Tick) -> Vec<&JournalEntry> {
        self.entries
            .iter()
            .filter(|e| match e {
                JournalEntry::Message { tick: t, .. } => *t >= tick,
                JournalEntry::TickBoundary { tick: t } => *t >= tick,
                JournalEntry::Snapshot { tick: t, .. } => *t >= tick,
                JournalEntry::Metadata { tick: t, .. } => *t >= tick,
            })
            .collect()
    }

    /// Get entries in a tick range (inclusive)
    pub fn entries_in_range(&self, start_tick: Tick, end_tick: Tick) -> Vec<&JournalEntry> {
        self.entries
            .iter()
            .filter(|e| {
                let t = match e {
                    JournalEntry::Message { tick, .. } => *tick,
                    JournalEntry::TickBoundary { tick } => *tick,
                    JournalEntry::Snapshot { tick, .. } => *tick,
                    JournalEntry::Metadata { tick, .. } => *tick,
                };
                t >= start_tick && t <= end_tick
            })
            .collect()
    }

    /// Get messages only
    pub fn messages(&self) -> impl Iterator<Item = (Tick, &Msg)> {
        self.entries.iter().filter_map(|e| match e {
            JournalEntry::Message { tick, msg, .. } => Some((*tick, msg)),
            _ => None,
        })
    }

    /// Get all snapshots
    pub fn snapshots(&self) -> &[Snapshot] {
        &self.snapshots
    }

    /// Get the nearest snapshot before or at a tick
    pub fn snapshot_at_or_before(&self, tick: Tick) -> Option<&Snapshot> {
        self.snapshots
            .iter()
            .filter(|s| s.tick <= tick)
            .max_by_key(|s| s.tick)
    }

    /// Get a specific snapshot by ID
    pub fn get_snapshot(&self, id: SnapshotId) -> Option<&Snapshot> {
        self.snapshots.iter().find(|s| s.id == id)
    }

    /// Clear all entries and snapshots
    pub fn clear(&mut self) {
        self.entries.clear();
        self.snapshots.clear();
        self.current_seq = 0;
        self.last_recorded_tick = None;
    }

    /// Get statistics about the journal
    pub fn stats(&self) -> JournalStats {
        let message_count = self
            .entries
            .iter()
            .filter(|e| matches!(e, JournalEntry::Message { .. }))
            .count();
        let tick_count = self
            .entries
            .iter()
            .filter(|e| matches!(e, JournalEntry::TickBoundary { .. }))
            .count();

        JournalStats {
            total_entries: self.entries.len(),
            message_count,
            tick_count,
            snapshot_count: self.snapshots.len(),
            first_tick: self.entries.first().map(|e| match e {
                JournalEntry::Message { tick, .. } => *tick,
                JournalEntry::TickBoundary { tick } => *tick,
                JournalEntry::Snapshot { tick, .. } => *tick,
                JournalEntry::Metadata { tick, .. } => *tick,
            }),
            last_tick: self.last_recorded_tick,
        }
    }

    fn enforce_limits(&mut self) {
        if self.config.max_entries > 0 && self.entries.len() > self.config.max_entries {
            let excess = self.entries.len() - self.config.max_entries;
            self.entries.drain(0..excess);
        }
    }

    fn enforce_snapshot_limits(&mut self) {
        if self.config.max_snapshots > 0 && self.snapshots.len() > self.config.max_snapshots {
            let excess = self.snapshots.len() - self.config.max_snapshots;
            self.snapshots.drain(0..excess);
        }
    }
}

impl Default for Journal {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about the journal
#[derive(Debug, Clone)]
pub struct JournalStats {
    /// Total number of entries
    pub total_entries: usize,
    /// Number of message entries
    pub message_count: usize,
    /// Number of tick boundaries
    pub tick_count: usize,
    /// Number of snapshots
    pub snapshot_count: usize,
    /// First tick recorded
    pub first_tick: Option<Tick>,
    /// Last tick recorded
    pub last_tick: Option<Tick>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_journal_recording() {
        let mut journal = Journal::new();
        journal.start_recording();

        let msg = Msg::tick(1);
        journal.record_message(1, msg.clone());
        journal.record_message(1, msg.clone());
        journal.record_message(2, msg.clone());

        let stats = journal.stats();
        assert_eq!(stats.message_count, 3);
        assert_eq!(stats.tick_count, 2); // Two tick boundaries
    }

    #[test]
    fn test_journal_disabled() {
        let mut journal = Journal::new();
        // Recording disabled by default

        let msg = Msg::tick(1);
        journal.record_message(1, msg);

        assert!(journal.entries().is_empty());
    }

    #[test]
    fn test_journal_snapshot() {
        let mut journal = Journal::new();
        let model = Model::new();

        let id = journal.take_snapshot(&model);

        assert_eq!(journal.snapshots().len(), 1);
        assert!(journal.get_snapshot(id).is_some());
    }

    #[test]
    fn test_entries_in_range() {
        let mut journal = Journal::new();
        journal.start_recording();

        let msg = Msg::tick(0);
        for tick in 0..10 {
            journal.record_message(tick, msg.clone());
        }

        let range = journal.entries_in_range(3, 6);
        assert!(!range.is_empty());

        // All entries in range should have tick 3-6
        for entry in range {
            match entry {
                JournalEntry::Message { tick, .. } => {
                    assert!(*tick >= 3 && *tick <= 6);
                }
                JournalEntry::TickBoundary { tick } => {
                    assert!(*tick >= 3 && *tick <= 6);
                }
                _ => {}
            }
        }
    }

    #[test]
    fn test_max_entries_limit() {
        let config = JournalConfig {
            recording_enabled: true,
            max_entries: 5,
            ..Default::default()
        };
        let mut journal = Journal::with_config(config);

        let msg = Msg::tick(0);
        for tick in 0..10 {
            journal.record_message(tick, msg.clone());
        }

        // Should be limited to 5 entries
        assert!(journal.entries().len() <= 5);
    }

    #[test]
    fn test_metadata_recording() {
        let mut journal = Journal::new();
        journal.start_recording();

        journal.record_metadata(1, "user", "alice");
        journal.record_metadata(1, "action", "login");

        let metadata: Vec<_> = journal
            .entries()
            .iter()
            .filter_map(|e| match e {
                JournalEntry::Metadata { key, value, .. } => Some((key.as_str(), value.as_str())),
                _ => None,
            })
            .collect();

        assert_eq!(metadata.len(), 2);
        assert!(metadata.contains(&("user", "alice")));
        assert!(metadata.contains(&("action", "login")));
    }
}
