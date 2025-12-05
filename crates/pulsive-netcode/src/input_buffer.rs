//! Input buffering for network synchronization
//!
//! Manages pending inputs that have been sent to the server but not yet confirmed.

use pulsive_core::Msg;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// An entry in the input buffer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputEntry {
    /// The tick when this input was generated
    pub tick: u64,
    /// The input message
    pub msg: Msg,
    /// Whether this input has been acknowledged by the server
    pub acknowledged: bool,
}

impl InputEntry {
    /// Create a new input entry
    pub fn new(tick: u64, msg: Msg) -> Self {
        Self {
            tick,
            msg,
            acknowledged: false,
        }
    }
}

/// Buffer for managing pending inputs
///
/// Stores inputs that have been sent to the server but not yet confirmed.
/// Used for client-side prediction and reconciliation.
#[derive(Debug)]
pub struct InputBuffer {
    /// Pending inputs (oldest first)
    inputs: VecDeque<InputEntry>,
    /// Maximum number of inputs to buffer
    capacity: usize,
    /// Last tick that was acknowledged by the server
    last_acknowledged_tick: u64,
}

impl InputBuffer {
    /// Create a new input buffer with the given capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            inputs: VecDeque::with_capacity(capacity),
            capacity,
            last_acknowledged_tick: 0,
        }
    }

    /// Add an input to the buffer
    ///
    /// Returns `Err` if the buffer is full.
    pub fn push(&mut self, tick: u64, msg: Msg) -> crate::Result<()> {
        if self.inputs.len() >= self.capacity {
            return Err(crate::Error::InputBufferFull);
        }
        self.inputs.push_back(InputEntry::new(tick, msg));
        Ok(())
    }

    /// Acknowledge all inputs up to and including the given tick
    ///
    /// Removes acknowledged inputs from the buffer.
    pub fn acknowledge(&mut self, tick: u64) {
        self.last_acknowledged_tick = tick;
        // Remove all inputs at or before the acknowledged tick
        while let Some(front) = self.inputs.front() {
            if front.tick <= tick {
                self.inputs.pop_front();
            } else {
                break;
            }
        }
    }

    /// Get all unacknowledged inputs
    pub fn unacknowledged(&self) -> impl Iterator<Item = &InputEntry> {
        self.inputs.iter().filter(|e| !e.acknowledged)
    }

    /// Get all inputs after a certain tick (for replay during reconciliation)
    pub fn inputs_after(&self, tick: u64) -> impl Iterator<Item = &InputEntry> {
        self.inputs.iter().filter(move |e| e.tick > tick)
    }

    /// Get the oldest unacknowledged tick
    pub fn oldest_unacknowledged_tick(&self) -> Option<u64> {
        self.inputs.front().map(|e| e.tick)
    }

    /// Get the newest input tick
    pub fn newest_tick(&self) -> Option<u64> {
        self.inputs.back().map(|e| e.tick)
    }

    /// Get the last acknowledged tick
    pub fn last_acknowledged_tick(&self) -> u64 {
        self.last_acknowledged_tick
    }

    /// Get the number of pending inputs
    pub fn len(&self) -> usize {
        self.inputs.len()
    }

    /// Check if the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty()
    }

    /// Check if the buffer is full
    pub fn is_full(&self) -> bool {
        self.inputs.len() >= self.capacity
    }

    /// Clear all inputs
    pub fn clear(&mut self) {
        self.inputs.clear();
    }

    /// Get the capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn make_msg(tick: u64) -> Msg {
        Msg::tick(tick)
    }

    #[test]
    fn test_push_and_len() {
        let mut buffer = InputBuffer::new(10);

        buffer.push(1, make_msg(1)).unwrap();
        buffer.push(2, make_msg(2)).unwrap();
        buffer.push(3, make_msg(3)).unwrap();

        assert_eq!(buffer.len(), 3);
        assert_eq!(buffer.oldest_unacknowledged_tick(), Some(1));
        assert_eq!(buffer.newest_tick(), Some(3));
    }

    #[test]
    fn test_acknowledge() {
        let mut buffer = InputBuffer::new(10);

        buffer.push(1, make_msg(1)).unwrap();
        buffer.push(2, make_msg(2)).unwrap();
        buffer.push(3, make_msg(3)).unwrap();

        buffer.acknowledge(2);

        assert_eq!(buffer.len(), 1);
        assert_eq!(buffer.oldest_unacknowledged_tick(), Some(3));
        assert_eq!(buffer.last_acknowledged_tick(), 2);
    }

    #[test]
    fn test_inputs_after() {
        let mut buffer = InputBuffer::new(10);

        buffer.push(1, make_msg(1)).unwrap();
        buffer.push(2, make_msg(2)).unwrap();
        buffer.push(3, make_msg(3)).unwrap();
        buffer.push(4, make_msg(4)).unwrap();

        let after_2: Vec<_> = buffer.inputs_after(2).collect();
        assert_eq!(after_2.len(), 2);
        assert_eq!(after_2[0].tick, 3);
        assert_eq!(after_2[1].tick, 4);
    }

    #[test]
    fn test_capacity() {
        let mut buffer = InputBuffer::new(3);

        buffer.push(1, make_msg(1)).unwrap();
        buffer.push(2, make_msg(2)).unwrap();
        buffer.push(3, make_msg(3)).unwrap();

        assert!(buffer.is_full());
        assert!(buffer.push(4, make_msg(4)).is_err());
    }
}
