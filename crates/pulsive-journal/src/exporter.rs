//! Export journal data to various formats

use crate::{Error, Result};
use pulsive_core::{Journal, JournalEntry, Tick};
use serde::Serialize;
use std::io::Write;

/// Export format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// RON format (Rust Object Notation)
    Ron,
    /// JSON format (requires serde_json feature)
    Json,
    /// CSV format (messages only)
    Csv,
    /// Human-readable text format
    Text,
}

/// Exporter for journal data
pub struct Exporter<'a> {
    journal: &'a Journal,
}

impl<'a> Exporter<'a> {
    /// Create a new exporter
    pub fn new(journal: &'a Journal) -> Self {
        Self { journal }
    }

    /// Export to a string in the specified format
    pub fn export(&self, format: ExportFormat) -> Result<String> {
        match format {
            ExportFormat::Ron => self.to_ron(),
            ExportFormat::Json => self.to_json(),
            ExportFormat::Csv => self.to_csv(),
            ExportFormat::Text => Ok(self.to_text()),
        }
    }

    /// Export to a writer
    pub fn export_to<W: Write>(&self, writer: &mut W, format: ExportFormat) -> Result<()> {
        let content = self.export(format)?;
        writer
            .write_all(content.as_bytes())
            .map_err(|e| Error::ExportError(e.to_string()))?;
        Ok(())
    }

    /// Export to RON format
    pub fn to_ron(&self) -> Result<String> {
        let export = ExportData::from_journal(self.journal);
        ron::ser::to_string_pretty(&export, ron::ser::PrettyConfig::default())
            .map_err(|e| Error::Serialization(e.to_string()))
    }

    /// Export to JSON format
    #[cfg(feature = "serde_json")]
    pub fn to_json(&self) -> Result<String> {
        let export = ExportData::from_journal(self.journal);
        serde_json::to_string_pretty(&export).map_err(|e| Error::Serialization(e.to_string()))
    }

    #[cfg(not(feature = "serde_json"))]
    pub fn to_json(&self) -> Result<String> {
        Err(Error::ExportError(
            "JSON export requires the 'serde_json' feature".to_string(),
        ))
    }

    /// Export to CSV format (messages only)
    pub fn to_csv(&self) -> Result<String> {
        let mut output = String::new();
        output.push_str("tick,seq,kind,event_id,actor,params\n");

        for entry in self.journal.entries() {
            if let JournalEntry::Message { tick, msg, seq } = entry {
                let kind = format!("{:?}", msg.kind);
                let event_id = msg
                    .event_id
                    .as_ref()
                    .map(|id| id.to_string())
                    .unwrap_or_default();
                let actor = msg
                    .actor
                    .as_ref()
                    .map(|a| a.raw().to_string())
                    .unwrap_or_default();
                let params = format!("{:?}", msg.params);

                // Escape CSV fields
                let params_escaped = params.replace('"', "\"\"");

                output.push_str(&format!(
                    "{},{},{},{},{},\"{}\"\n",
                    tick, seq, kind, event_id, actor, params_escaped
                ));
            }
        }

        Ok(output)
    }

    /// Export to human-readable text format
    pub fn to_text(&self) -> String {
        let mut output = String::new();
        let stats = self.journal.stats();

        output.push_str("=== Journal Export ===\n\n");
        output.push_str(&format!("Total entries: {}\n", stats.total_entries));
        output.push_str(&format!("Messages: {}\n", stats.message_count));
        output.push_str(&format!("Ticks: {}\n", stats.tick_count));
        output.push_str(&format!("Snapshots: {}\n", stats.snapshot_count));

        if let (Some(first), Some(last)) = (stats.first_tick, stats.last_tick) {
            output.push_str(&format!("Tick range: {} - {}\n", first, last));
        }

        output.push_str("\n=== Entries ===\n\n");

        let mut current_tick: Option<Tick> = None;

        for entry in self.journal.entries() {
            match entry {
                JournalEntry::TickBoundary { tick } => {
                    if current_tick != Some(*tick) {
                        output.push_str(&format!("\n--- Tick {} ---\n", tick));
                        current_tick = Some(*tick);
                    }
                }
                JournalEntry::Message { tick, msg, seq } => {
                    if current_tick != Some(*tick) {
                        output.push_str(&format!("\n--- Tick {} ---\n", tick));
                        current_tick = Some(*tick);
                    }
                    let kind = format!("{:?}", msg.kind);
                    let event_id = msg
                        .event_id
                        .as_ref()
                        .map(|id| format!(" [{}]", id))
                        .unwrap_or_default();
                    let actor = msg
                        .actor
                        .as_ref()
                        .map(|a| format!(" actor={}", a.raw()))
                        .unwrap_or_default();

                    output.push_str(&format!("  #{} {}{}{}\n", seq, kind, event_id, actor));

                    if !msg.params.is_empty() {
                        output.push_str(&format!("      params: {:?}\n", msg.params));
                    }
                }
                JournalEntry::Snapshot { tick, snapshot_id } => {
                    output.push_str(&format!(
                        "  [SNAPSHOT] id={} at tick {}\n",
                        snapshot_id.0, tick
                    ));
                }
                JournalEntry::Metadata { tick, key, value } => {
                    output.push_str(&format!("  [META] {}={} at tick {}\n", key, value, tick));
                }
            }
        }

        output
    }

    /// Export only entries in a tick range
    pub fn export_range(&self, start: Tick, end: Tick, format: ExportFormat) -> Result<String> {
        let entries: Vec<_> = self
            .journal
            .entries_in_range(start, end)
            .into_iter()
            .cloned()
            .collect();
        let filtered_journal = FilteredExport { entries };

        match format {
            ExportFormat::Ron => {
                ron::ser::to_string_pretty(&filtered_journal, ron::ser::PrettyConfig::default())
                    .map_err(|e| Error::Serialization(e.to_string()))
            }
            #[cfg(feature = "serde_json")]
            ExportFormat::Json => serde_json::to_string_pretty(&filtered_journal)
                .map_err(|e| Error::Serialization(e.to_string())),
            #[cfg(not(feature = "serde_json"))]
            ExportFormat::Json => Err(Error::ExportError(
                "JSON export requires the 'serde_json' feature".to_string(),
            )),
            _ => Err(Error::ExportError(
                "Range export only supports RON and JSON".to_string(),
            )),
        }
    }
}

/// Data structure for full journal export
#[derive(Debug, Clone, Serialize)]
struct ExportData {
    version: u32,
    stats: ExportStats,
    entries: Vec<JournalEntry>,
}

impl ExportData {
    fn from_journal(journal: &Journal) -> Self {
        let stats = journal.stats();
        Self {
            version: 1,
            stats: ExportStats {
                total_entries: stats.total_entries,
                message_count: stats.message_count,
                tick_count: stats.tick_count,
                snapshot_count: stats.snapshot_count,
                first_tick: stats.first_tick,
                last_tick: stats.last_tick,
            },
            entries: journal.entries().to_vec(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct ExportStats {
    total_entries: usize,
    message_count: usize,
    tick_count: usize,
    snapshot_count: usize,
    first_tick: Option<Tick>,
    last_tick: Option<Tick>,
}

#[derive(Debug, Clone, Serialize)]
struct FilteredExport {
    entries: Vec<JournalEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsive_core::{JournalConfig, Model, Runtime};

    fn create_test_journal() -> Journal {
        let mut model = Model::new();
        let mut runtime = Runtime::new();
        let mut journal = Journal::with_config(JournalConfig {
            recording_enabled: true,
            snapshot_interval: 5,
            ..Default::default()
        });

        for _ in 0..10 {
            runtime.tick_with_journal(&mut model, &mut journal);
        }

        journal.record_metadata(5, "test_key", "test_value");
        journal
    }

    #[test]
    fn test_export_ron() {
        let journal = create_test_journal();
        let exporter = Exporter::new(&journal);
        let ron = exporter.to_ron().unwrap();

        assert!(ron.contains("version"));
        assert!(ron.contains("entries"));
    }

    #[test]
    fn test_export_csv() {
        let journal = create_test_journal();
        let exporter = Exporter::new(&journal);
        let csv = exporter.to_csv().unwrap();

        assert!(csv.starts_with("tick,seq,kind,event_id,actor,params\n"));
        assert!(csv.lines().count() > 1);
    }

    #[test]
    fn test_export_text() {
        let journal = create_test_journal();
        let exporter = Exporter::new(&journal);
        let text = exporter.to_text();

        assert!(text.contains("Journal Export"));
        assert!(text.contains("Tick"));
    }

    #[test]
    fn test_export_range() {
        let journal = create_test_journal();
        let exporter = Exporter::new(&journal);
        let ron = exporter.export_range(3, 6, ExportFormat::Ron).unwrap();

        assert!(ron.contains("entries"));
    }
}
