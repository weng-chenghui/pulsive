//! Replay functionality for journal data

#![allow(dead_code)] // Public API that will be used by consumers

use crate::Result;
use pulsive_core::{Journal, JournalEntry, Model, Msg, Runtime};

/// Speed for replay playback
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ReplaySpeed {
    /// Step one tick at a time (manual)
    #[default]
    Step,
    /// Real-time playback (1 tick per second, adjustable)
    RealTime(f64),
    /// As fast as possible
    Instant,
}

/// State of the replayer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayState {
    /// Not started
    Idle,
    /// Currently playing
    Playing,
    /// Paused
    Paused,
    /// Reached end of journal
    Finished,
}

/// Replayer for journal data
///
/// Provides fine-grained control over replaying recorded sessions:
/// - Goto specific tick
/// - Step forward/backward
/// - Play at various speeds
/// - Seek to snapshots
pub struct Replayer<'a> {
    journal: &'a Journal,
    state: ReplayState,
    speed: ReplaySpeed,
    current_tick: u64,
    target_tick: Option<u64>,
}

impl<'a> Replayer<'a> {
    /// Create a new replayer for a journal
    pub fn new(journal: &'a Journal) -> Self {
        Self {
            journal,
            state: ReplayState::Idle,
            speed: ReplaySpeed::default(),
            current_tick: 0,
            target_tick: None,
        }
    }

    /// Get the current state
    pub fn state(&self) -> ReplayState {
        self.state
    }

    /// Get the current tick
    pub fn current_tick(&self) -> u64 {
        self.current_tick
    }

    /// Get the replay speed
    pub fn speed(&self) -> ReplaySpeed {
        self.speed
    }

    /// Set the replay speed
    pub fn set_speed(&mut self, speed: ReplaySpeed) {
        self.speed = speed;
    }

    /// Get the first tick in the journal
    pub fn first_tick(&self) -> Option<u64> {
        self.journal.stats().first_tick
    }

    /// Get the last tick in the journal
    pub fn last_tick(&self) -> Option<u64> {
        self.journal.stats().last_tick
    }

    /// Go to a specific tick
    ///
    /// This will restore from the nearest snapshot and replay messages
    pub fn goto(&mut self, model: &mut Model, runtime: &mut Runtime, tick: u64) -> Result<()> {
        // Find nearest snapshot
        let snapshot = self.journal.snapshot_at_or_before(tick);

        if let Some(snapshot) = snapshot {
            // Restore from snapshot
            *model = snapshot.model.clone();
            self.current_tick = snapshot.tick;
        } else {
            // Start from beginning
            *model = Model::new();
            self.current_tick = 0;
        }

        // Replay messages from current tick to target
        if tick > self.current_tick {
            self.replay_range(model, runtime, self.current_tick, tick)?;
        }

        self.current_tick = tick;
        self.state = ReplayState::Paused;
        Ok(())
    }

    /// Step forward one tick
    pub fn step_forward(&mut self, model: &mut Model, runtime: &mut Runtime) -> Result<bool> {
        let last_tick = self.last_tick().unwrap_or(0);
        if self.current_tick >= last_tick {
            self.state = ReplayState::Finished;
            return Ok(false);
        }

        let next_tick = self.current_tick + 1;
        self.replay_range(model, runtime, self.current_tick, next_tick)?;
        self.current_tick = next_tick;

        if self.current_tick >= last_tick {
            self.state = ReplayState::Finished;
        }

        Ok(true)
    }

    /// Step backward one tick (requires snapshots)
    pub fn step_backward(&mut self, model: &mut Model, runtime: &mut Runtime) -> Result<bool> {
        if self.current_tick == 0 {
            return Ok(false);
        }

        let target = self.current_tick - 1;
        self.goto(model, runtime, target)?;
        Ok(true)
    }

    /// Start playing
    pub fn play(&mut self) {
        if self.state != ReplayState::Finished {
            self.state = ReplayState::Playing;
        }
    }

    /// Pause playback
    pub fn pause(&mut self) {
        if self.state == ReplayState::Playing {
            self.state = ReplayState::Paused;
        }
    }

    /// Reset to the beginning
    pub fn reset(&mut self, model: &mut Model) {
        *model = Model::new();
        self.current_tick = 0;
        self.state = ReplayState::Idle;
    }

    /// Seek to the nearest snapshot at or before a tick
    pub fn seek_to_snapshot(&mut self, model: &mut Model, tick: u64) -> Result<Option<u64>> {
        if let Some(snapshot) = self.journal.snapshot_at_or_before(tick) {
            *model = snapshot.model.clone();
            self.current_tick = snapshot.tick;
            self.state = ReplayState::Paused;
            Ok(Some(snapshot.tick))
        } else {
            Ok(None)
        }
    }

    /// Get messages for a specific tick
    pub fn messages_at(&self, tick: u64) -> Vec<&Msg> {
        self.journal
            .entries()
            .iter()
            .filter_map(|e| match e {
                JournalEntry::Message { tick: t, msg, .. } if *t == tick => Some(msg),
                _ => None,
            })
            .collect()
    }

    /// Get all available snapshot ticks
    pub fn snapshot_ticks(&self) -> Vec<u64> {
        self.journal.snapshots().iter().map(|s| s.tick).collect()
    }

    /// Replay a range of ticks
    fn replay_range(
        &self,
        model: &mut Model,
        runtime: &mut Runtime,
        start: u64,
        end: u64,
    ) -> Result<()> {
        let entries = self.journal.entries_in_range(start, end);

        for entry in entries {
            if let JournalEntry::Message { msg, tick, .. } = entry {
                if *tick > start && *tick <= end {
                    // Queue the message
                    runtime.send(msg.clone());
                }
            }
        }

        // Process all queued messages
        runtime.process_queue(model);
        Ok(())
    }
}

/// Builder for creating replay sessions
pub struct ReplaySessionBuilder<'a> {
    journal: &'a Journal,
    start_tick: Option<u64>,
    end_tick: Option<u64>,
    speed: ReplaySpeed,
}

impl<'a> ReplaySessionBuilder<'a> {
    /// Create a new session builder
    pub fn new(journal: &'a Journal) -> Self {
        Self {
            journal,
            start_tick: None,
            end_tick: None,
            speed: ReplaySpeed::default(),
        }
    }

    /// Set the starting tick
    pub fn start_at(mut self, tick: u64) -> Self {
        self.start_tick = Some(tick);
        self
    }

    /// Set the ending tick
    pub fn end_at(mut self, tick: u64) -> Self {
        self.end_tick = Some(tick);
        self
    }

    /// Set the replay speed
    pub fn with_speed(mut self, speed: ReplaySpeed) -> Self {
        self.speed = speed;
        self
    }

    /// Build the replayer
    pub fn build(self) -> Replayer<'a> {
        let mut replayer = Replayer::new(self.journal);
        replayer.speed = self.speed;
        if let Some(start) = self.start_tick {
            replayer.current_tick = start;
        }
        replayer.target_tick = self.end_tick;
        replayer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsive_core::{Journal, JournalConfig, Model, Runtime};

    fn create_recorded_session() -> (Journal, Model) {
        let mut model = Model::new();
        let mut runtime = Runtime::new();
        let mut journal = Journal::with_config(JournalConfig {
            recording_enabled: true,
            snapshot_interval: 5,
            ..Default::default()
        });

        // Record 20 ticks
        for _ in 0..20 {
            runtime.tick_with_journal(&mut model, &mut journal);
        }

        (journal, model)
    }

    #[test]
    fn test_replayer_goto() {
        let (journal, _) = create_recorded_session();
        let mut model = Model::new();
        let mut runtime = Runtime::new();
        let mut replayer = Replayer::new(&journal);

        replayer.goto(&mut model, &mut runtime, 10).unwrap();
        assert_eq!(replayer.current_tick(), 10);
        assert_eq!(replayer.state(), ReplayState::Paused);
    }

    #[test]
    fn test_replayer_step() {
        let (journal, _) = create_recorded_session();
        let mut model = Model::new();
        let mut runtime = Runtime::new();
        let mut replayer = Replayer::new(&journal);

        // Step forward
        replayer.step_forward(&mut model, &mut runtime).unwrap();
        assert_eq!(replayer.current_tick(), 1);

        replayer.step_forward(&mut model, &mut runtime).unwrap();
        assert_eq!(replayer.current_tick(), 2);
    }

    #[test]
    fn test_replayer_reset() {
        let (journal, _) = create_recorded_session();
        let mut model = Model::new();
        let mut runtime = Runtime::new();
        let mut replayer = Replayer::new(&journal);

        replayer.goto(&mut model, &mut runtime, 10).unwrap();
        replayer.reset(&mut model);

        assert_eq!(replayer.current_tick(), 0);
        assert_eq!(replayer.state(), ReplayState::Idle);
    }

    #[test]
    fn test_snapshot_ticks() {
        let (journal, _) = create_recorded_session();
        let replayer = Replayer::new(&journal);
        let ticks = replayer.snapshot_ticks();

        // Should have snapshots at intervals of 5
        assert!(!ticks.is_empty());
    }
}
