//! State interpolation for smooth rendering
//!
//! Interpolates between two model states to produce smooth visual transitions,
//! even when the simulation runs at a lower tick rate than the render rate.

use pulsive_core::{Model, StateHistory, StateInterpolation, Value};

/// Interpolator for smooth state transitions
///
/// Stores previous and current states and interpolates between them
/// based on the render time.
#[derive(Debug)]
pub struct Interpolator {
    /// Previous state (for interpolation)
    prev_state: Option<(u64, Model)>,
    /// Current state (target)
    curr_state: Option<(u64, Model)>,
}

impl Interpolator {
    /// Create a new interpolator
    pub fn new() -> Self {
        Self {
            prev_state: None,
            curr_state: None,
        }
    }

    /// Update with a new authoritative state
    pub fn push_state(&mut self, tick: u64, model: Model) {
        // Shift current to previous
        self.prev_state = self.curr_state.take();
        self.curr_state = Some((tick, model));
    }

    /// Get the interpolated state at a given render time
    ///
    /// `alpha` is the interpolation factor:
    /// - 0.0 = use previous state
    /// - 1.0 = use current state
    /// - 0.5 = halfway between
    pub fn interpolate(&self, alpha: f32) -> Option<Model> {
        match (&self.prev_state, &self.curr_state) {
            (Some((_, prev)), Some((_, curr))) => Some(Self::interpolate_models(prev, curr, alpha)),
            (None, Some((_, curr))) => Some(curr.clone()),
            (Some((_, prev)), None) => Some(prev.clone()),
            (None, None) => None,
        }
    }

    /// Interpolate using a StateHistory
    ///
    /// Finds the appropriate states from history and interpolates between them.
    pub fn interpolate_from_history<H: StateHistory>(
        &self,
        history: &H,
        target_tick: u64,
        sub_tick_fraction: f32,
    ) -> Option<Model> {
        // Get interpolation states from history
        let (before_tick, before, after_tick, after) =
            history.get_interpolation_states(target_tick)?;

        // Calculate interpolation factor
        let base_alpha = if before_tick == after_tick {
            0.0
        } else {
            let range = (after_tick - before_tick) as f32;
            let offset = (target_tick - before_tick) as f32 + sub_tick_fraction;
            (offset / range).clamp(0.0, 1.0)
        };

        Some(Self::interpolate_models(before, after, base_alpha))
    }

    /// Interpolate between two models
    ///
    /// This is a basic implementation that interpolates numeric properties.
    /// For more complex interpolation (positions, rotations), users should
    /// implement their own interpolation logic.
    fn interpolate_models(prev: &Model, curr: &Model, alpha: f32) -> Model {
        let mut result = curr.clone();
        let alpha_f64 = alpha as f64;

        // Interpolate entity properties
        for entity in result.entities_mut().iter_mut() {
            let entity_id = entity.id;

            // Try to find corresponding entity in previous state
            if let Some(prev_entity) = prev.entities().get(entity_id) {
                // Interpolate numeric properties
                for (key, curr_value) in entity.properties.iter_mut() {
                    if let Some(prev_value) = prev_entity.get(key) {
                        *curr_value = Self::interpolate_value(prev_value, curr_value, alpha_f64);
                    }
                }
            }
        }

        // Interpolate global properties
        for (key, curr_value) in result.globals_mut().iter_mut() {
            if let Some(prev_value) = prev.globals().get(key) {
                *curr_value = Self::interpolate_value(prev_value, curr_value, alpha_f64);
            }
        }

        result
    }

    /// Interpolate between two values
    fn interpolate_value(prev: &Value, curr: &Value, alpha: f64) -> Value {
        match (prev, curr) {
            (Value::Float(p), Value::Float(c)) => Value::Float(p + (c - p) * alpha),
            (Value::Int(p), Value::Int(c)) => {
                // Interpolate as float, round to int
                let interpolated = *p as f64 + (*c - *p) as f64 * alpha;
                Value::Int(interpolated.round() as i64)
            }
            // For non-numeric types, use current value
            _ => curr.clone(),
        }
    }

    /// Get the current tick
    pub fn current_tick(&self) -> Option<u64> {
        self.curr_state.as_ref().map(|(t, _)| *t)
    }

    /// Get the previous tick
    pub fn previous_tick(&self) -> Option<u64> {
        self.prev_state.as_ref().map(|(t, _)| *t)
    }

    /// Check if interpolation is possible (have both states)
    pub fn can_interpolate(&self) -> bool {
        self.prev_state.is_some() && self.curr_state.is_some()
    }

    /// Reset the interpolator
    pub fn reset(&mut self) {
        self.prev_state = None;
        self.curr_state = None;
    }
}

impl Default for Interpolator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interpolate_values() {
        // Float interpolation
        let prev = Value::Float(0.0);
        let curr = Value::Float(10.0);

        let mid = Interpolator::interpolate_value(&prev, &curr, 0.5);
        assert_eq!(mid, Value::Float(5.0));

        let quarter = Interpolator::interpolate_value(&prev, &curr, 0.25);
        assert_eq!(quarter, Value::Float(2.5));

        // Int interpolation
        let prev_int = Value::Int(0);
        let curr_int = Value::Int(10);

        let mid_int = Interpolator::interpolate_value(&prev_int, &curr_int, 0.5);
        assert_eq!(mid_int, Value::Int(5));
    }

    #[test]
    fn test_push_and_interpolate() {
        let mut interpolator = Interpolator::new();

        let mut model1 = Model::new();
        model1.set_global("value", 0.0f64);

        let mut model2 = Model::new();
        model2.set_global("value", 10.0f64);

        interpolator.push_state(0, model1);
        assert!(!interpolator.can_interpolate());

        interpolator.push_state(1, model2);
        assert!(interpolator.can_interpolate());

        let result = interpolator.interpolate(0.5).unwrap();
        assert_eq!(
            result.get_global("value").and_then(|v| v.as_float()),
            Some(5.0)
        );
    }
}
