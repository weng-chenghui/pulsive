//! Client-side prediction engine
//!
//! Applies inputs locally before server confirmation for responsive gameplay.
//! Works with any StateHistory implementation for state storage.

use crate::{InputBuffer, InputEntry, Result};
use pulsive_core::{Model, Msg, Runtime, StateHistory};

/// Client-side prediction engine
///
/// Stores predicted states and pending inputs, allowing for:
/// - Immediate local response to player input
/// - Rollback and replay when server state differs
///
/// Generic over `H: StateHistory` to allow different storage backends.
pub struct PredictionEngine<H: StateHistory> {
    /// State history for rollback
    history: H,
    /// Pending inputs not yet confirmed by server
    input_buffer: InputBuffer,
    /// Last tick confirmed by the server
    last_server_tick: u64,
    /// Current predicted tick (may be ahead of server)
    predicted_tick: u64,
}

impl<H: StateHistory> PredictionEngine<H> {
    /// Create a new prediction engine
    pub fn new(history: H) -> Self {
        Self {
            history,
            input_buffer: InputBuffer::new(256), // Default capacity
            last_server_tick: 0,
            predicted_tick: 0,
        }
    }

    /// Create with custom input buffer capacity
    pub fn with_input_capacity(history: H, capacity: usize) -> Self {
        Self {
            history,
            input_buffer: InputBuffer::new(capacity),
            last_server_tick: 0,
            predicted_tick: 0,
        }
    }

    /// Predict a local input
    ///
    /// Applies the input immediately to the local state and stores it
    /// for potential replay during reconciliation.
    pub fn predict(&mut self, model: &mut Model, runtime: &mut Runtime, input: Msg) -> Result<()> {
        // Save current state before prediction
        self.history.save_state(self.predicted_tick, model);

        // Buffer the input for reconciliation
        self.input_buffer.push(self.predicted_tick, input.clone())?;

        // Apply input to local state
        runtime.send(input);
        runtime.process_queue(model);

        // Advance predicted tick
        self.predicted_tick += 1;

        Ok(())
    }

    /// Advance prediction by one tick without input
    ///
    /// Used when the simulation needs to advance but the player
    /// hasn't provided input this frame.
    pub fn advance(&mut self, model: &mut Model, runtime: &mut Runtime) {
        // Save state
        self.history.save_state(self.predicted_tick, model);

        // Run one tick
        runtime.tick(model);

        // Advance predicted tick
        self.predicted_tick += 1;
    }

    /// Reconcile with authoritative server state
    ///
    /// If the server state differs from our predicted state at that tick,
    /// rolls back to the server state and replays all inputs since then.
    pub fn reconcile(
        &mut self,
        model: &mut Model,
        runtime: &mut Runtime,
        server_state: &Model,
        server_tick: u64,
    ) -> Result<bool> {
        // Acknowledge inputs up to server tick
        self.input_buffer.acknowledge(server_tick);
        self.last_server_tick = server_tick;

        // Check if we need to reconcile
        if server_tick >= self.predicted_tick {
            // Server is ahead or equal, just use server state
            *model = server_state.clone();
            self.predicted_tick = server_tick;
            return Ok(false);
        }

        // Get our predicted state at the server tick
        let our_state = self.history.get_state(server_tick);

        // Compare states (simple comparison - can be made more sophisticated)
        let needs_reconcile = match our_state {
            Some(our) => !Self::states_match(our, server_state),
            None => true, // No state means we need to reconcile
        };

        if needs_reconcile {
            // Rollback to server state
            *model = server_state.clone();

            // Clear history up to server tick
            self.history.clear_before(server_tick);

            // Replay all inputs since server tick
            let inputs_to_replay: Vec<InputEntry> = self
                .input_buffer
                .inputs_after(server_tick)
                .cloned()
                .collect();

            for input in inputs_to_replay {
                self.history.save_state(input.tick, model);
                runtime.send(input.msg);
                runtime.process_queue(model);
            }

            // Update predicted tick
            self.predicted_tick = self
                .input_buffer
                .newest_tick()
                .map(|t| t + 1)
                .unwrap_or(server_tick);
        }

        Ok(needs_reconcile)
    }

    /// Compare two states for equality
    ///
    /// This is a simple comparison. For production use, you may want
    /// to compare only relevant properties or use checksums.
    fn states_match(a: &Model, b: &Model) -> bool {
        // Compare tick
        if a.current_tick() != b.current_tick() {
            return false;
        }

        // Compare globals (simplified)
        if a.globals().len() != b.globals().len() {
            return false;
        }

        // For a more robust comparison, you'd compare entities and their properties
        // This is a simplified version
        true
    }

    /// Get the current predicted tick
    pub fn predicted_tick(&self) -> u64 {
        self.predicted_tick
    }

    /// Get the last server tick
    pub fn last_server_tick(&self) -> u64 {
        self.last_server_tick
    }

    /// Get the number of ticks we're ahead of the server
    pub fn prediction_frames(&self) -> u64 {
        self.predicted_tick.saturating_sub(self.last_server_tick)
    }

    /// Get the number of pending inputs
    pub fn pending_inputs(&self) -> usize {
        self.input_buffer.len()
    }

    /// Get access to the state history
    pub fn history(&self) -> &H {
        &self.history
    }

    /// Get mutable access to the state history
    pub fn history_mut(&mut self) -> &mut H {
        &mut self.history
    }

    /// Reset the prediction engine
    pub fn reset(&mut self) {
        self.history.clear();
        self.input_buffer.clear();
        self.last_server_tick = 0;
        self.predicted_tick = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Simple in-memory history for testing
    struct TestHistory {
        states: Vec<(u64, Model)>,
    }

    impl TestHistory {
        fn new() -> Self {
            Self { states: Vec::new() }
        }
    }

    impl StateHistory for TestHistory {
        fn save_state(&mut self, tick: u64, model: &Model) {
            self.states.push((tick, model.clone()));
        }

        fn get_state(&self, tick: u64) -> Option<&Model> {
            self.states.iter().find(|(t, _)| *t == tick).map(|(_, m)| m)
        }

        fn get_nearest_before(&self, tick: u64) -> Option<(u64, &Model)> {
            self.states
                .iter()
                .filter(|(t, _)| *t <= tick)
                .max_by_key(|(t, _)| *t)
                .map(|(t, m)| (*t, m))
        }

        fn get_nearest_after(&self, tick: u64) -> Option<(u64, &Model)> {
            self.states
                .iter()
                .filter(|(t, _)| *t >= tick)
                .min_by_key(|(t, _)| *t)
                .map(|(t, m)| (*t, m))
        }

        fn clear_before(&mut self, tick: u64) {
            self.states.retain(|(t, _)| *t >= tick);
        }

        fn clear(&mut self) {
            self.states.clear();
        }

        fn capacity(&self) -> Option<usize> {
            None
        }

        fn len(&self) -> usize {
            self.states.len()
        }

        fn tick_range(&self) -> Option<(u64, u64)> {
            if self.states.is_empty() {
                None
            } else {
                let ticks: Vec<_> = self.states.iter().map(|(t, _)| *t).collect();
                Some((*ticks.iter().min().unwrap(), *ticks.iter().max().unwrap()))
            }
        }
    }

    #[test]
    fn test_predict() {
        let history = TestHistory::new();
        let mut engine = PredictionEngine::new(history);
        let mut model = Model::new();
        let mut runtime = Runtime::new();

        // Predict an input
        let input = Msg::tick(0);
        engine.predict(&mut model, &mut runtime, input).unwrap();

        assert_eq!(engine.predicted_tick(), 1);
        assert_eq!(engine.pending_inputs(), 1);
    }

    #[test]
    fn test_reconcile_server_ahead() {
        let history = TestHistory::new();
        let mut engine = PredictionEngine::new(history);
        let mut model = Model::new();
        let mut runtime = Runtime::new();

        // Predict a few frames
        for _ in 0..3 {
            engine.advance(&mut model, &mut runtime);
        }

        // Server is ahead of us (tick 5 vs our tick 3)
        let server_state = Model::new();
        let reconciled = engine
            .reconcile(&mut model, &mut runtime, &server_state, 5)
            .unwrap();

        // Should not need reconciliation since server is ahead
        assert!(!reconciled);
        assert_eq!(engine.predicted_tick(), 5);
    }
}
