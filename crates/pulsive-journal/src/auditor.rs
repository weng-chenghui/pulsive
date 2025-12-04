//! Auditing and analytics for journal data

use pulsive_core::{ActorId, DefId, Journal, JournalEntry, MsgKind};
use std::collections::HashMap;

/// Auditor for querying and analyzing journal data
pub struct Auditor<'a> {
    journal: &'a Journal,
}

impl<'a> Auditor<'a> {
    /// Create a new auditor for a journal
    pub fn new(journal: &'a Journal) -> Self {
        Self { journal }
    }

    /// Generate a comprehensive audit report
    pub fn generate_report(&self) -> AuditReport {
        let stats = self.journal.stats();
        let mut event_counts: HashMap<String, u64> = HashMap::new();
        let mut actor_actions: HashMap<u64, u64> = HashMap::new();
        let mut commands_by_type: HashMap<String, u64> = HashMap::new();

        for entry in self.journal.entries() {
            if let JournalEntry::Message { msg, .. } = entry {
                // Count by event type
                let event_name = match &msg.kind {
                    MsgKind::Tick => "Tick".to_string(),
                    MsgKind::Command => "Command".to_string(),
                    MsgKind::Event => "Event".to_string(),
                    MsgKind::ScheduledEvent => "ScheduledEvent".to_string(),
                    MsgKind::EntitySpawned => "EntitySpawned".to_string(),
                    MsgKind::EntityDestroyed => "EntityDestroyed".to_string(),
                    MsgKind::PropertyChanged => "PropertyChanged".to_string(),
                    MsgKind::FlagAdded => "FlagAdded".to_string(),
                    MsgKind::FlagRemoved => "FlagRemoved".to_string(),
                    MsgKind::Custom(id) => format!("Custom({})", id),
                };
                *event_counts.entry(event_name).or_insert(0) += 1;

                // Count actor actions
                if let Some(actor) = &msg.actor {
                    *actor_actions.entry(actor.raw()).or_insert(0) += 1;
                }

                // Count commands by type
                if msg.kind == MsgKind::Command {
                    if let Some(event_id) = &msg.event_id {
                        *commands_by_type.entry(event_id.to_string()).or_insert(0) += 1;
                    }
                }
            }
        }

        AuditReport {
            total_entries: stats.total_entries,
            total_messages: stats.message_count,
            total_ticks: stats.tick_count,
            snapshot_count: stats.snapshot_count,
            first_tick: stats.first_tick,
            last_tick: stats.last_tick,
            event_counts,
            actor_actions,
            commands_by_type,
        }
    }

    /// Query entries matching specific criteria
    pub fn query(&self, query: &AuditQuery) -> Vec<&JournalEntry> {
        self.journal
            .entries()
            .iter()
            .filter(|entry| self.matches_query(entry, query))
            .collect()
    }

    /// Get a summary of events for a specific actor
    pub fn actor_summary(&self, actor_id: ActorId) -> EventSummary {
        let mut total = 0;
        let mut by_type: HashMap<String, u64> = HashMap::new();

        for entry in self.journal.entries() {
            if let JournalEntry::Message { msg, .. } = entry {
                if msg.actor.as_ref().map(|a| a.raw()) == Some(actor_id.raw()) {
                    total += 1;
                    if let Some(event_id) = &msg.event_id {
                        *by_type.entry(event_id.to_string()).or_insert(0) += 1;
                    }
                }
            }
        }

        EventSummary { total, by_type }
    }

    /// Get events in a time range
    pub fn events_in_range(&self, start_tick: u64, end_tick: u64) -> Vec<&JournalEntry> {
        self.journal.entries_in_range(start_tick, end_tick)
    }

    /// Count occurrences of a specific event type
    pub fn count_event(&self, event_id: &DefId) -> u64 {
        self.journal
            .entries()
            .iter()
            .filter(|entry| {
                if let JournalEntry::Message { msg, .. } = entry {
                    msg.event_id.as_ref() == Some(event_id)
                } else {
                    false
                }
            })
            .count() as u64
    }

    /// Get all unique event IDs
    pub fn unique_events(&self) -> Vec<DefId> {
        let mut events: Vec<DefId> = self
            .journal
            .entries()
            .iter()
            .filter_map(|entry| {
                if let JournalEntry::Message { msg, .. } = entry {
                    msg.event_id.clone()
                } else {
                    None
                }
            })
            .collect();

        events.sort_by_key(|a| a.to_string());
        events.dedup();
        events
    }

    /// Get metadata entries
    pub fn metadata(&self) -> Vec<(&str, &str, u64)> {
        self.journal
            .entries()
            .iter()
            .filter_map(|entry| {
                if let JournalEntry::Metadata { tick, key, value } = entry {
                    Some((key.as_str(), value.as_str(), *tick))
                } else {
                    None
                }
            })
            .collect()
    }

    fn matches_query(&self, entry: &JournalEntry, query: &AuditQuery) -> bool {
        match entry {
            JournalEntry::Message { tick, msg, .. } => {
                // Check tick range
                if let Some(start) = query.start_tick {
                    if *tick < start {
                        return false;
                    }
                }
                if let Some(end) = query.end_tick {
                    if *tick > end {
                        return false;
                    }
                }

                // Check actor filter
                if let Some(actor_id) = query.actor_id {
                    if msg.actor.as_ref().map(|a| a.raw()) != Some(actor_id) {
                        return false;
                    }
                }

                // Check event type filter
                if let Some(ref event_id) = query.event_id {
                    if msg.event_id.as_ref() != Some(event_id) {
                        return false;
                    }
                }

                // Check message kind filter
                if let Some(ref kind) = query.msg_kind {
                    if &msg.kind != kind {
                        return false;
                    }
                }

                true
            }
            JournalEntry::TickBoundary { tick } => {
                if !query.include_tick_boundaries {
                    return false;
                }
                if let Some(start) = query.start_tick {
                    if *tick < start {
                        return false;
                    }
                }
                if let Some(end) = query.end_tick {
                    if *tick > end {
                        return false;
                    }
                }
                true
            }
            JournalEntry::Snapshot { tick, .. } => {
                if !query.include_snapshots {
                    return false;
                }
                if let Some(start) = query.start_tick {
                    if *tick < start {
                        return false;
                    }
                }
                if let Some(end) = query.end_tick {
                    if *tick > end {
                        return false;
                    }
                }
                true
            }
            JournalEntry::Metadata { tick, key, .. } => {
                if !query.include_metadata {
                    return false;
                }
                if let Some(start) = query.start_tick {
                    if *tick < start {
                        return false;
                    }
                }
                if let Some(end) = query.end_tick {
                    if *tick > end {
                        return false;
                    }
                }
                if let Some(ref filter_key) = query.metadata_key {
                    if key != filter_key {
                        return false;
                    }
                }
                true
            }
        }
    }
}

/// A comprehensive audit report
#[derive(Debug, Clone)]
pub struct AuditReport {
    /// Total number of journal entries
    pub total_entries: usize,
    /// Total number of messages
    pub total_messages: usize,
    /// Total number of ticks
    pub total_ticks: usize,
    /// Number of snapshots
    pub snapshot_count: usize,
    /// First tick in journal
    pub first_tick: Option<u64>,
    /// Last tick in journal
    pub last_tick: Option<u64>,
    /// Count of each event type
    pub event_counts: HashMap<String, u64>,
    /// Actions by actor
    pub actor_actions: HashMap<u64, u64>,
    /// Commands grouped by type
    pub commands_by_type: HashMap<String, u64>,
}

impl std::fmt::Display for AuditReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== Audit Report ===")?;
        writeln!(f, "Total entries: {}", self.total_entries)?;
        writeln!(f, "Total messages: {}", self.total_messages)?;
        writeln!(f, "Total ticks: {}", self.total_ticks)?;
        writeln!(f, "Snapshots: {}", self.snapshot_count)?;

        if let (Some(first), Some(last)) = (self.first_tick, self.last_tick) {
            writeln!(f, "Tick range: {} - {}", first, last)?;
        }

        if !self.event_counts.is_empty() {
            writeln!(f, "\nEvents by type:")?;
            let mut sorted: Vec<_> = self.event_counts.iter().collect();
            sorted.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
            for (event, count) in sorted {
                writeln!(f, "  {}: {}", event, count)?;
            }
        }

        if !self.actor_actions.is_empty() {
            writeln!(f, "\nActions by actor:")?;
            for (actor, count) in &self.actor_actions {
                writeln!(f, "  Actor {}: {}", actor, count)?;
            }
        }

        if !self.commands_by_type.is_empty() {
            writeln!(f, "\nCommands by type:")?;
            for (cmd, count) in &self.commands_by_type {
                writeln!(f, "  {}: {}", cmd, count)?;
            }
        }

        Ok(())
    }
}

/// Query criteria for filtering journal entries
#[derive(Debug, Clone, Default)]
pub struct AuditQuery {
    /// Start tick (inclusive)
    pub start_tick: Option<u64>,
    /// End tick (inclusive)
    pub end_tick: Option<u64>,
    /// Filter by actor
    pub actor_id: Option<u64>,
    /// Filter by event ID
    pub event_id: Option<DefId>,
    /// Filter by message kind
    pub msg_kind: Option<MsgKind>,
    /// Include tick boundaries in results
    pub include_tick_boundaries: bool,
    /// Include snapshots in results
    pub include_snapshots: bool,
    /// Include metadata in results
    pub include_metadata: bool,
    /// Filter metadata by key
    pub metadata_key: Option<String>,
}

impl AuditQuery {
    /// Create a new empty query
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by tick range
    pub fn in_range(mut self, start: u64, end: u64) -> Self {
        self.start_tick = Some(start);
        self.end_tick = Some(end);
        self
    }

    /// Filter by actor
    pub fn by_actor(mut self, actor_id: u64) -> Self {
        self.actor_id = Some(actor_id);
        self
    }

    /// Filter by event ID
    pub fn by_event(mut self, event_id: impl Into<DefId>) -> Self {
        self.event_id = Some(event_id.into());
        self
    }

    /// Filter by message kind
    pub fn by_kind(mut self, kind: MsgKind) -> Self {
        self.msg_kind = Some(kind);
        self
    }

    /// Include tick boundaries
    pub fn with_tick_boundaries(mut self) -> Self {
        self.include_tick_boundaries = true;
        self
    }

    /// Include snapshots
    pub fn with_snapshots(mut self) -> Self {
        self.include_snapshots = true;
        self
    }

    /// Include metadata
    pub fn with_metadata(mut self) -> Self {
        self.include_metadata = true;
        self
    }

    /// Filter metadata by key
    pub fn metadata_with_key(mut self, key: impl Into<String>) -> Self {
        self.include_metadata = true;
        self.metadata_key = Some(key.into());
        self
    }
}

/// Summary of events for an entity or actor
#[derive(Debug, Clone)]
pub struct EventSummary {
    /// Total number of events
    pub total: u64,
    /// Events grouped by type
    pub by_type: HashMap<String, u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsive_core::{EntityRef, Msg};

    fn create_test_journal() -> Journal {
        let mut journal = Journal::new();
        journal.start_recording();

        // Record some messages
        for tick in 0..10 {
            journal.record_tick(tick);
            journal.record_message(tick, Msg::tick(tick));

            if tick % 2 == 0 {
                journal.record_message(tick, Msg::event("test_event", EntityRef::Global, tick));
            }
        }

        journal.record_metadata(5, "user", "test_user");
        journal
    }

    #[test]
    fn test_generate_report() {
        let journal = create_test_journal();
        let auditor = Auditor::new(&journal);
        let report = auditor.generate_report();

        assert!(report.total_messages > 0);
        assert!(report.event_counts.contains_key("Tick"));
    }

    #[test]
    fn test_query_by_range() {
        let journal = create_test_journal();
        let auditor = Auditor::new(&journal);

        let query = AuditQuery::new().in_range(3, 6);
        let results = auditor.query(&query);

        for entry in results {
            if let JournalEntry::Message { tick, .. } = entry {
                assert!(*tick >= 3 && *tick <= 6);
            }
        }
    }

    #[test]
    fn test_unique_events() {
        let journal = create_test_journal();
        let auditor = Auditor::new(&journal);
        let events = auditor.unique_events();

        assert!(!events.is_empty());
    }

    #[test]
    fn test_metadata() {
        let journal = create_test_journal();
        let auditor = Auditor::new(&journal);
        let metadata = auditor.metadata();

        assert!(!metadata.is_empty());
        assert!(metadata
            .iter()
            .any(|(k, v, _)| *k == "user" && *v == "test_user"));
    }
}
