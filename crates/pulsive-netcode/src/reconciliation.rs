//! Server state reconciliation
//!
//! Handles correcting client state when server authoritative state arrives.

use crate::Result;
use pulsive_core::{Model, Msg, Runtime, StateHistory};

/// Reconciler for applying server corrections
///
/// Provides utilities for comparing states and applying corrections
/// without full prediction engine overhead.
pub struct Reconciler<H: StateHistory> {
    /// State history for rollback
    history: H,
    /// Last confirmed server tick
    last_server_tick: u64,
}

impl<H: StateHistory> Reconciler<H> {
    /// Create a new reconciler
    pub fn new(history: H) -> Self {
        Self {
            history,
            last_server_tick: 0,
        }
    }

    /// Apply a server state correction
    ///
    /// Replaces the local state with the server state and clears
    /// history before the server tick.
    pub fn apply_correction(&mut self, model: &mut Model, server_state: &Model, server_tick: u64) {
        *model = server_state.clone();
        self.history.clear_before(server_tick);
        self.last_server_tick = server_tick;
    }

    /// Rollback to a previous state
    ///
    /// Restores the model to the state at the given tick.
    /// Returns the tick that was actually restored (may differ if exact tick not found).
    pub fn rollback(&self, model: &mut Model, target_tick: u64) -> Result<u64> {
        // Try exact tick first
        if let Some(state) = self.history.get_state(target_tick) {
            *model = state.clone();
            return Ok(target_tick);
        }

        // Fall back to nearest before
        if let Some((actual_tick, state)) = self.history.get_nearest_before(target_tick) {
            *model = state.clone();
            return Ok(actual_tick);
        }

        Err(crate::Error::StateNotFound(target_tick))
    }

    /// Rollback and replay inputs
    ///
    /// Rolls back to the target tick, then replays the given inputs.
    pub fn rollback_and_replay(
        &mut self,
        model: &mut Model,
        runtime: &mut Runtime,
        target_tick: u64,
        inputs: &[Msg],
    ) -> Result<()> {
        // Rollback to target tick
        let _actual_tick = self.rollback(model, target_tick)?;

        // Replay inputs
        for input in inputs {
            runtime.send(input.clone());
            runtime.process_queue(model);
        }

        Ok(())
    }

    /// Save the current state
    pub fn save_state(&mut self, tick: u64, model: &Model) {
        self.history.save_state(tick, model);
    }

    /// Get the last server tick
    pub fn last_server_tick(&self) -> u64 {
        self.last_server_tick
    }

    /// Get access to the history
    pub fn history(&self) -> &H {
        &self.history
    }

    /// Get mutable access to the history
    pub fn history_mut(&mut self) -> &mut H {
        &mut self.history
    }
}

/// State comparison utilities
#[allow(dead_code)]
pub mod compare {
    use pulsive_core::{Model, Value};

    /// Compare two models and return whether they match
    pub fn states_equal(a: &Model, b: &Model) -> bool {
        // Compare ticks
        if a.current_tick() != b.current_tick() {
            return false;
        }

        // Compare globals
        if !maps_equal(&a.globals, &b.globals) {
            return false;
        }

        // Compare entity count
        if a.entities.len() != b.entities.len() {
            return false;
        }

        // Compare each entity
        for entity_a in a.entities.iter() {
            match b.entities.get(entity_a.id) {
                Some(entity_b) => {
                    if entity_a.kind != entity_b.kind {
                        return false;
                    }
                    if !maps_equal(&entity_a.properties, &entity_b.properties) {
                        return false;
                    }
                }
                None => return false,
            }
        }

        true
    }

    /// Compare two value maps
    fn maps_equal(a: &pulsive_core::ValueMap, b: &pulsive_core::ValueMap) -> bool {
        if a.len() != b.len() {
            return false;
        }

        for (key, value_a) in a.iter() {
            match b.get(key) {
                Some(value_b) => {
                    if !values_equal(value_a, value_b) {
                        return false;
                    }
                }
                None => return false,
            }
        }

        true
    }

    /// Compare two values with tolerance for floats
    fn values_equal(a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Float(f1), Value::Float(f2)) => {
                // Use epsilon comparison for floats
                (f1 - f2).abs() < 1e-6
            }
            _ => a == b,
        }
    }

    /// Compute a simple checksum of a model for quick comparison
    pub fn state_checksum(model: &Model) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();

        // Hash tick
        model.current_tick().hash(&mut hasher);

        // Hash globals (sorted for consistency)
        let mut globals: Vec<_> = model.globals.iter().collect();
        globals.sort_by_key(|(k, _)| *k);
        for (key, value) in globals {
            key.hash(&mut hasher);
            // Hash value representation
            format!("{:?}", value).hash(&mut hasher);
        }

        // Hash entity count
        model.entities.len().hash(&mut hasher);

        hasher.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Simple test history
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
            self.states.retain(|(t, _)| *t != tick);
            self.states.push((tick, model.clone()));
            self.states.sort_by_key(|(t, _)| *t);
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
                let min = self.states.first().map(|(t, _)| *t).unwrap();
                let max = self.states.last().map(|(t, _)| *t).unwrap();
                Some((min, max))
            }
        }
    }

    #[test]
    fn test_apply_correction() {
        let history = TestHistory::new();
        let mut reconciler = Reconciler::new(history);
        let mut model = Model::new();
        model.set_global("value", 100i64);

        let mut server_state = Model::new();
        server_state.set_global("value", 200i64);

        reconciler.apply_correction(&mut model, &server_state, 10);

        assert_eq!(
            model.get_global("value").and_then(|v| v.as_int()),
            Some(200)
        );
        assert_eq!(reconciler.last_server_tick(), 10);
    }

    #[test]
    fn test_rollback() {
        let mut history = TestHistory::new();
        let mut model = Model::new();
        model.set_global("tick", 5i64);
        history.save_state(5, &model);

        model.set_global("tick", 10i64);
        history.save_state(10, &model);

        let reconciler = Reconciler::new(history);
        let mut target = Model::new();

        let actual = reconciler.rollback(&mut target, 5).unwrap();
        assert_eq!(actual, 5);
        assert_eq!(target.get_global("tick").and_then(|v| v.as_int()), Some(5));
    }

    #[test]
    fn test_state_comparison() {
        let mut a = Model::new();
        a.set_global("value", 100i64);

        let mut b = Model::new();
        b.set_global("value", 100i64);

        assert!(compare::states_equal(&a, &b));

        b.set_global("value", 200i64);
        assert!(!compare::states_equal(&a, &b));
    }

    #[test]
    fn test_checksum() {
        let mut a = Model::new();
        a.set_global("value", 100i64);

        let mut b = Model::new();
        b.set_global("value", 100i64);

        assert_eq!(compare::state_checksum(&a), compare::state_checksum(&b));

        b.set_global("value", 200i64);
        assert_ne!(compare::state_checksum(&a), compare::state_checksum(&b));
    }
}
