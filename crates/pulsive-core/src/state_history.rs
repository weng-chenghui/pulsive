//! State history trait for storing and retrieving historical model states
//!
//! This trait is used by:
//! - `pulsive-netcode` for prediction and rollback
//! - `pulsive-rollback-buffer` for real-time gaming (bounded ring buffer)
//! - `pulsive-journal` for debugging and auditing (unbounded history)
//!
//! # Example
//!
//! ```rust,ignore
//! use pulsive_core::{Model, StateHistory};
//!
//! struct MyHistory {
//!     states: Vec<(u64, Model)>,
//! }
//!
//! impl StateHistory for MyHistory {
//!     fn save_state(&mut self, tick: u64, model: &Model) {
//!         self.states.push((tick, model.clone()));
//!     }
//!     
//!     fn get_state(&self, tick: u64) -> Option<&Model> {
//!         self.states.iter().find(|(t, _)| *t == tick).map(|(_, m)| m)
//!     }
//!     
//!     // ... other methods
//! }
//! ```

use crate::Model;

/// Trait for storing and retrieving historical model states.
///
/// Implementations can choose different storage strategies:
/// - Ring buffer (bounded, fast, for real-time)
/// - Growing vector (unbounded, for auditing)
/// - Hybrid (snapshots + deltas)
pub trait StateHistory {
    /// Save a state snapshot at the given tick.
    ///
    /// The implementation decides whether to clone the model or store a reference.
    fn save_state(&mut self, tick: u64, model: &Model);

    /// Get the state at exactly the given tick, if it exists.
    fn get_state(&self, tick: u64) -> Option<&Model>;

    /// Get the state at or before the given tick.
    ///
    /// Returns `(actual_tick, model)` where `actual_tick <= tick`.
    /// This is useful for rollback when exact tick isn't available.
    fn get_nearest_before(&self, tick: u64) -> Option<(u64, &Model)>;

    /// Get the state at or after the given tick.
    ///
    /// Returns `(actual_tick, model)` where `actual_tick >= tick`.
    /// This is useful for forward interpolation.
    fn get_nearest_after(&self, tick: u64) -> Option<(u64, &Model)>;

    /// Clear all states before the given tick.
    ///
    /// Used to free memory when old states are no longer needed.
    fn clear_before(&mut self, tick: u64);

    /// Clear all stored states.
    fn clear(&mut self);

    /// Get the capacity of this history.
    ///
    /// Returns `None` for unbounded histories (like journal).
    /// Returns `Some(n)` for bounded histories (like ring buffer).
    fn capacity(&self) -> Option<usize>;

    /// Get the number of states currently stored.
    fn len(&self) -> usize;

    /// Check if the history is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the tick range of stored states.
    ///
    /// Returns `None` if no states are stored.
    /// Returns `Some((oldest_tick, newest_tick))` otherwise.
    fn tick_range(&self) -> Option<(u64, u64)>;
}

/// Extension trait for interpolation between states
pub trait StateInterpolation: StateHistory {
    /// Get two states for interpolation: the state before and after the target tick.
    ///
    /// Returns `None` if interpolation is not possible (missing states).
    /// Returns `Some((before_tick, before_model, after_tick, after_model))`.
    fn get_interpolation_states(&self, tick: u64) -> Option<(u64, &Model, u64, &Model)> {
        let before = self.get_nearest_before(tick)?;
        let after = self.get_nearest_after(tick)?;
        Some((before.0, before.1, after.0, after.1))
    }

    /// Calculate the interpolation factor between two ticks.
    ///
    /// Returns a value in [0.0, 1.0] where:
    /// - 0.0 means use the "before" state entirely
    /// - 1.0 means use the "after" state entirely
    /// - 0.5 means halfway between
    fn interpolation_factor(before_tick: u64, after_tick: u64, target_tick: u64) -> f32 {
        if before_tick == after_tick {
            return 0.0;
        }
        let range = (after_tick - before_tick) as f32;
        let offset = (target_tick - before_tick) as f32;
        (offset / range).clamp(0.0, 1.0)
    }
}

// Blanket implementation: any StateHistory can do interpolation
impl<T: StateHistory> StateInterpolation for T {}

#[cfg(test)]
mod tests {
    use super::*;

    // Simple in-memory implementation for testing
    struct SimpleHistory {
        states: Vec<(u64, Model)>,
    }

    impl SimpleHistory {
        fn new() -> Self {
            Self { states: Vec::new() }
        }
    }

    impl StateHistory for SimpleHistory {
        fn save_state(&mut self, tick: u64, model: &Model) {
            // Remove existing state at this tick if any
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
            None // Unbounded
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
    fn test_save_and_get() {
        let mut history = SimpleHistory::new();
        let model = Model::new();

        history.save_state(10, &model);
        history.save_state(20, &model);
        history.save_state(30, &model);

        assert!(history.get_state(10).is_some());
        assert!(history.get_state(20).is_some());
        assert!(history.get_state(15).is_none());
    }

    #[test]
    fn test_nearest_before() {
        let mut history = SimpleHistory::new();
        let model = Model::new();

        history.save_state(10, &model);
        history.save_state(20, &model);
        history.save_state(30, &model);

        let (tick, _) = history.get_nearest_before(25).unwrap();
        assert_eq!(tick, 20);

        let (tick, _) = history.get_nearest_before(10).unwrap();
        assert_eq!(tick, 10);

        assert!(history.get_nearest_before(5).is_none());
    }

    #[test]
    fn test_nearest_after() {
        let mut history = SimpleHistory::new();
        let model = Model::new();

        history.save_state(10, &model);
        history.save_state(20, &model);
        history.save_state(30, &model);

        let (tick, _) = history.get_nearest_after(15).unwrap();
        assert_eq!(tick, 20);

        let (tick, _) = history.get_nearest_after(30).unwrap();
        assert_eq!(tick, 30);

        assert!(history.get_nearest_after(35).is_none());
    }

    #[test]
    fn test_clear_before() {
        let mut history = SimpleHistory::new();
        let model = Model::new();

        history.save_state(10, &model);
        history.save_state(20, &model);
        history.save_state(30, &model);

        history.clear_before(20);

        assert!(history.get_state(10).is_none());
        assert!(history.get_state(20).is_some());
        assert!(history.get_state(30).is_some());
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn test_interpolation_factor() {
        assert_eq!(SimpleHistory::interpolation_factor(0, 10, 0), 0.0);
        assert_eq!(SimpleHistory::interpolation_factor(0, 10, 10), 1.0);
        assert_eq!(SimpleHistory::interpolation_factor(0, 10, 5), 0.5);
        assert_eq!(SimpleHistory::interpolation_factor(10, 10, 10), 0.0);
    }

    #[test]
    fn test_tick_range() {
        let mut history = SimpleHistory::new();
        let model = Model::new();

        assert!(history.tick_range().is_none());

        history.save_state(10, &model);
        assert_eq!(history.tick_range(), Some((10, 10)));

        history.save_state(30, &model);
        assert_eq!(history.tick_range(), Some((10, 30)));

        history.save_state(20, &model);
        assert_eq!(history.tick_range(), Some((10, 30)));
    }
}
