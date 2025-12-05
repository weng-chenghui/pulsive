//! Pulsive Rollback Buffer - Optimized ring buffer for real-time state history
//!
//! This crate provides a bounded, memory-efficient state history implementation
//! optimized for real-time applications like games.
//!
//! # Features
//!
//! - **Bounded memory**: Fixed-size ring buffer, no unbounded growth
//! - **O(1) insertion**: Constant time to save new states
//! - **Fast lookup**: Quick access to recent states
//! - **Automatic eviction**: Old states are automatically removed
//!
//! # Example
//!
//! ```rust
//! use pulsive_core::{Model, StateHistory};
//! use pulsive_rollback_buffer::RollbackBuffer;
//!
//! // Create a buffer that holds 128 frames of history
//! let mut buffer = RollbackBuffer::new(128);
//!
//! // Save states
//! let model = Model::new();
//! buffer.save_state(0, &model);
//! buffer.save_state(1, &model);
//! buffer.save_state(2, &model);
//!
//! // Retrieve a state
//! if let Some(state) = buffer.get_state(1) {
//!     println!("Got state at tick 1");
//! }
//!
//! // Get nearest state for rollback
//! if let Some((tick, state)) = buffer.get_nearest_before(5) {
//!     println!("Nearest state before tick 5 is at tick {}", tick);
//! }
//! ```

use pulsive_core::{Model, StateHistory};

/// A ring buffer for storing recent model states
///
/// Optimized for real-time applications where only recent history is needed.
/// Older states are automatically evicted when the buffer is full.
#[derive(Debug)]
pub struct RollbackBuffer {
    /// Ring buffer storage: (tick, model)
    /// None means the slot is empty
    states: Vec<Option<(u64, Model)>>,
    /// Current write position in the ring buffer
    head: usize,
    /// Number of states currently stored
    count: usize,
    /// Capacity (max states)
    capacity: usize,
}

impl RollbackBuffer {
    /// Create a new rollback buffer with the given capacity
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of states to store (typically 64-256 frames)
    ///
    /// # Example
    ///
    /// ```rust
    /// use pulsive_rollback_buffer::RollbackBuffer;
    ///
    /// // 128 frames at 60fps = ~2 seconds of history
    /// let buffer = RollbackBuffer::new(128);
    /// ```
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "Capacity must be greater than 0");
        Self {
            states: (0..capacity).map(|_| None).collect(),
            head: 0,
            count: 0,
            capacity,
        }
    }

    /// Get the index for a given tick (if it would be in the buffer)
    fn tick_to_index(&self, tick: u64) -> usize {
        (tick as usize) % self.capacity
    }

    /// Check if a tick is within the current valid range
    #[allow(dead_code)]
    fn is_tick_valid(&self, tick: u64) -> bool {
        if self.count == 0 {
            return false;
        }

        // Get the range of valid ticks
        if let Some((oldest, newest)) = self.tick_range() {
            tick >= oldest && tick <= newest
        } else {
            false
        }
    }

    /// Get all stored states as an iterator (oldest to newest)
    pub fn iter(&self) -> impl Iterator<Item = (u64, &Model)> {
        // Collect valid states and sort by tick
        let mut states: Vec<_> = self
            .states
            .iter()
            .filter_map(|s| s.as_ref().map(|(t, m)| (*t, m)))
            .collect();
        states.sort_by_key(|(t, _)| *t);
        states.into_iter()
    }

    /// Get statistics about the buffer
    pub fn stats(&self) -> BufferStats {
        let (oldest, newest) = self.tick_range().unwrap_or((0, 0));
        BufferStats {
            capacity: self.capacity,
            count: self.count,
            oldest_tick: oldest,
            newest_tick: newest,
        }
    }
}

impl StateHistory for RollbackBuffer {
    fn save_state(&mut self, tick: u64, model: &Model) {
        // Calculate index for this tick
        let index = self.tick_to_index(tick);

        // Check if we're overwriting an existing state
        let was_empty = self.states[index].is_none();

        // Store the state
        self.states[index] = Some((tick, model.clone()));

        // Update count if we used a new slot
        if was_empty && self.count < self.capacity {
            self.count += 1;
        }

        // Update head to point to next slot
        self.head = (index + 1) % self.capacity;
    }

    fn get_state(&self, tick: u64) -> Option<&Model> {
        let index = self.tick_to_index(tick);
        self.states[index]
            .as_ref()
            .filter(|(t, _)| *t == tick)
            .map(|(_, m)| m)
    }

    fn get_nearest_before(&self, tick: u64) -> Option<(u64, &Model)> {
        self.states
            .iter()
            .filter_map(|s| s.as_ref())
            .filter(|(t, _)| *t <= tick)
            .max_by_key(|(t, _)| *t)
            .map(|(t, m)| (*t, m))
    }

    fn get_nearest_after(&self, tick: u64) -> Option<(u64, &Model)> {
        self.states
            .iter()
            .filter_map(|s| s.as_ref())
            .filter(|(t, _)| *t >= tick)
            .min_by_key(|(t, _)| *t)
            .map(|(t, m)| (*t, m))
    }

    fn clear_before(&mut self, tick: u64) {
        for state in &mut self.states {
            if let Some((t, _)) = state {
                if *t < tick {
                    *state = None;
                    self.count = self.count.saturating_sub(1);
                }
            }
        }
    }

    fn clear(&mut self) {
        for state in &mut self.states {
            *state = None;
        }
        self.count = 0;
        self.head = 0;
    }

    fn capacity(&self) -> Option<usize> {
        Some(self.capacity)
    }

    fn len(&self) -> usize {
        self.count
    }

    fn tick_range(&self) -> Option<(u64, u64)> {
        if self.count == 0 {
            return None;
        }

        let mut min_tick = u64::MAX;
        let mut max_tick = 0u64;

        for (t, _) in self.states.iter().flatten() {
            min_tick = min_tick.min(*t);
            max_tick = max_tick.max(*t);
        }

        if min_tick == u64::MAX {
            None
        } else {
            Some((min_tick, max_tick))
        }
    }
}

impl Default for RollbackBuffer {
    fn default() -> Self {
        Self::new(128) // Default to 128 frames (~2 seconds at 60fps)
    }
}

/// Statistics about the rollback buffer
#[derive(Debug, Clone, Copy)]
pub struct BufferStats {
    /// Maximum capacity
    pub capacity: usize,
    /// Current number of stored states
    pub count: usize,
    /// Oldest tick in the buffer
    pub oldest_tick: u64,
    /// Newest tick in the buffer
    pub newest_tick: u64,
}

impl BufferStats {
    /// Get the tick range (newest - oldest)
    pub fn tick_range(&self) -> u64 {
        if self.count == 0 {
            0
        } else {
            self.newest_tick - self.oldest_tick
        }
    }

    /// Get the fill percentage (0.0 to 1.0)
    pub fn fill_ratio(&self) -> f32 {
        self.count as f32 / self.capacity as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let buffer = RollbackBuffer::new(64);
        assert_eq!(buffer.capacity(), Some(64));
        assert_eq!(buffer.len(), 0);
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_save_and_get() {
        let mut buffer = RollbackBuffer::new(64);
        let model = Model::new();

        buffer.save_state(10, &model);
        buffer.save_state(11, &model);
        buffer.save_state(12, &model);

        assert_eq!(buffer.len(), 3);
        assert!(buffer.get_state(10).is_some());
        assert!(buffer.get_state(11).is_some());
        assert!(buffer.get_state(12).is_some());
        assert!(buffer.get_state(13).is_none());
    }

    #[test]
    fn test_ring_buffer_wrap() {
        let mut buffer = RollbackBuffer::new(4);
        let model = Model::new();

        // Fill the buffer
        buffer.save_state(0, &model);
        buffer.save_state(1, &model);
        buffer.save_state(2, &model);
        buffer.save_state(3, &model);
        assert_eq!(buffer.len(), 4);

        // Add more - should wrap and overwrite
        buffer.save_state(4, &model);
        buffer.save_state(5, &model);

        // Old states should be gone
        assert!(buffer.get_state(0).is_none());
        assert!(buffer.get_state(1).is_none());

        // New states should exist
        assert!(buffer.get_state(4).is_some());
        assert!(buffer.get_state(5).is_some());
    }

    #[test]
    fn test_nearest_before() {
        let mut buffer = RollbackBuffer::new(64);
        let model = Model::new();

        buffer.save_state(10, &model);
        buffer.save_state(20, &model);
        buffer.save_state(30, &model);

        let (tick, _) = buffer.get_nearest_before(25).unwrap();
        assert_eq!(tick, 20);

        let (tick, _) = buffer.get_nearest_before(30).unwrap();
        assert_eq!(tick, 30);

        let (tick, _) = buffer.get_nearest_before(35).unwrap();
        assert_eq!(tick, 30);
    }

    #[test]
    fn test_nearest_after() {
        let mut buffer = RollbackBuffer::new(64);
        let model = Model::new();

        buffer.save_state(10, &model);
        buffer.save_state(20, &model);
        buffer.save_state(30, &model);

        let (tick, _) = buffer.get_nearest_after(15).unwrap();
        assert_eq!(tick, 20);

        let (tick, _) = buffer.get_nearest_after(10).unwrap();
        assert_eq!(tick, 10);

        assert!(buffer.get_nearest_after(35).is_none());
    }

    #[test]
    fn test_clear_before() {
        let mut buffer = RollbackBuffer::new(64);
        let model = Model::new();

        buffer.save_state(10, &model);
        buffer.save_state(20, &model);
        buffer.save_state(30, &model);

        buffer.clear_before(20);

        assert!(buffer.get_state(10).is_none());
        assert!(buffer.get_state(20).is_some());
        assert!(buffer.get_state(30).is_some());
    }

    #[test]
    fn test_tick_range() {
        let mut buffer = RollbackBuffer::new(64);
        let model = Model::new();

        assert!(buffer.tick_range().is_none());

        buffer.save_state(10, &model);
        assert_eq!(buffer.tick_range(), Some((10, 10)));

        buffer.save_state(30, &model);
        assert_eq!(buffer.tick_range(), Some((10, 30)));

        buffer.save_state(20, &model);
        assert_eq!(buffer.tick_range(), Some((10, 30)));
    }

    #[test]
    fn test_stats() {
        let mut buffer = RollbackBuffer::new(64);
        let model = Model::new();

        buffer.save_state(10, &model);
        buffer.save_state(20, &model);
        buffer.save_state(30, &model);

        let stats = buffer.stats();
        assert_eq!(stats.capacity, 64);
        assert_eq!(stats.count, 3);
        assert_eq!(stats.oldest_tick, 10);
        assert_eq!(stats.newest_tick, 30);
        assert_eq!(stats.tick_range(), 20);
    }
}
