//! Time system for tick-based simulation
//!
//! Provides discrete time management for deterministic systems:
//! - `Tick` - Logical time unit
//! - `Speed` - Processing rate control
//! - `Clock` - Simulation clock with state
//! - `Timestamp` - Human-readable date representation

use serde::{Deserialize, Serialize};
use std::fmt;

/// A discrete tick identifier (logical time unit)
pub type Tick = u64;

/// Processing speed settings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Speed {
    /// System is paused
    #[default]
    Paused,
    /// Slowest speed
    VerySlow,
    /// Slow speed
    Slow,
    /// Normal speed
    Normal,
    /// Fast speed
    Fast,
    /// Fastest speed
    VeryFast,
}

impl Speed {
    /// Get the tick interval in milliseconds for this speed
    /// Returns None if paused
    pub fn tick_interval_ms(&self) -> Option<u64> {
        match self {
            Speed::Paused => None,
            Speed::VerySlow => Some(2000),
            Speed::Slow => Some(1000),
            Speed::Normal => Some(500),
            Speed::Fast => Some(200),
            Speed::VeryFast => Some(50),
        }
    }

    /// Check if the system is paused
    pub fn is_paused(&self) -> bool {
        matches!(self, Speed::Paused)
    }
}

/// Simulation clock state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clock {
    /// Current tick number
    pub tick: Tick,
    /// Current processing speed
    pub speed: Speed,
    /// Start timestamp (for display purposes)
    pub start_date: Timestamp,
    /// Ticks per day (for timestamp calculation)
    pub ticks_per_day: u32,
}

impl Clock {
    /// Create a new clock
    pub fn new() -> Self {
        Self {
            tick: 0,
            speed: Speed::Paused,
            start_date: Timestamp::new(1, 1, 1),
            ticks_per_day: 1,
        }
    }

    /// Create with a specific start timestamp
    pub fn with_start_date(year: i32, month: u8, day: u8) -> Self {
        Self {
            tick: 0,
            speed: Speed::Paused,
            start_date: Timestamp::new(year, month, day),
            ticks_per_day: 1,
        }
    }

    /// Advance to the next tick
    pub fn advance(&mut self) {
        self.tick += 1;
    }

    /// Get the current timestamp
    pub fn current_date(&self) -> Timestamp {
        let days_elapsed = (self.tick / self.ticks_per_day as u64) as i32;
        self.start_date.add_days(days_elapsed)
    }

    /// Set the processing speed
    pub fn set_speed(&mut self, speed: Speed) {
        self.speed = speed;
    }

    /// Toggle pause
    pub fn toggle_pause(&mut self, previous_speed: Speed) -> Speed {
        if self.speed.is_paused() {
            self.speed = if previous_speed.is_paused() {
                Speed::Normal
            } else {
                previous_speed
            };
        } else {
            self.speed = Speed::Paused;
        }
        self.speed
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::new()
    }
}

/// A timestamp representing a calendar date (year, month, day)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timestamp {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

impl Timestamp {
    /// Days in each month (non-leap year)
    const DAYS_IN_MONTH: [u8; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

    /// Create a new timestamp
    pub fn new(year: i32, month: u8, day: u8) -> Self {
        Self { year, month, day }
    }

    /// Check if this year is a leap year
    pub fn is_leap_year(year: i32) -> bool {
        (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
    }

    /// Get days in a specific month
    pub fn days_in_month(year: i32, month: u8) -> u8 {
        if month == 2 && Self::is_leap_year(year) {
            29
        } else {
            Self::DAYS_IN_MONTH[(month - 1) as usize]
        }
    }

    /// Add days to this timestamp
    pub fn add_days(&self, days: i32) -> Self {
        let mut year = self.year;
        let mut month = self.month;
        let mut day = self.day as i32;

        day += days;

        // Handle positive overflow
        while day > Self::days_in_month(year, month) as i32 {
            day -= Self::days_in_month(year, month) as i32;
            month += 1;
            if month > 12 {
                month = 1;
                year += 1;
            }
        }

        // Handle negative underflow
        while day < 1 {
            month -= 1;
            if month < 1 {
                month = 12;
                year -= 1;
            }
            day += Self::days_in_month(year, month) as i32;
        }

        Self {
            year,
            month,
            day: day as u8,
        }
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clock() {
        let mut clock = Clock::with_start_date(2000, 1, 1);
        assert_eq!(clock.tick, 0);
        assert_eq!(clock.current_date().to_string(), "2000-01-01");

        clock.advance();
        assert_eq!(clock.tick, 1);
        assert_eq!(clock.current_date().to_string(), "2000-01-02");
    }

    #[test]
    fn test_timestamp_add_days() {
        let date = Timestamp::new(2000, 1, 1);
        assert_eq!(date.add_days(30).to_string(), "2000-01-31");
        assert_eq!(date.add_days(366).to_string(), "2001-01-01"); // 2000 is leap year
    }

    #[test]
    fn test_speed() {
        assert!(Speed::Paused.is_paused());
        assert!(!Speed::Normal.is_paused());
        assert_eq!(Speed::Normal.tick_interval_ms(), Some(500));
        assert_eq!(Speed::Paused.tick_interval_ms(), None);
    }
}
