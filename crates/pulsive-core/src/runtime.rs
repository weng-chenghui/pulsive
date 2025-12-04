//! Elm-style runtime for the reactive engine

use crate::{
    effect::EffectResult, expr::EvalContext, Cmd, DefId, Effect, EntityRef, Expr, Model, Msg,
    MsgKind, Value, ValueMap,
};
use std::collections::VecDeque;

/// Result of an update cycle
#[derive(Debug, Clone)]
pub struct UpdateResult {
    /// Commands to execute
    pub cmd: Cmd,
    /// Messages emitted during this update
    pub emitted_messages: Vec<Msg>,
    /// Effect results (spawned entities, logs, etc.)
    pub effect_result: EffectResult,
}

impl UpdateResult {
    /// Create an empty result
    pub fn new() -> Self {
        Self {
            cmd: Cmd::None,
            emitted_messages: Vec::new(),
            effect_result: EffectResult::new(),
        }
    }

    /// Create with a command
    pub fn with_cmd(cmd: Cmd) -> Self {
        Self {
            cmd,
            emitted_messages: Vec::new(),
            effect_result: EffectResult::new(),
        }
    }
}

impl Default for UpdateResult {
    fn default() -> Self {
        Self::new()
    }
}

/// The main runtime that processes messages and updates the model
pub struct Runtime {
    /// Pending messages to process
    message_queue: VecDeque<Msg>,
    /// Scheduled messages (tick, msg)
    scheduled: Vec<(u64, Msg)>,
    /// Event handlers registered by event ID
    event_handlers: Vec<EventHandler>,
    /// Tick handlers (run every tick)
    tick_handlers: Vec<TickHandler>,
}

/// An event handler that responds to specific events
#[derive(Clone)]
pub struct EventHandler {
    /// Which event this handles
    pub event_id: DefId,
    /// Condition for this handler to run
    pub condition: Option<Expr>,
    /// Effects to execute
    pub effects: Vec<Effect>,
    /// Priority (higher = runs first)
    pub priority: i32,
}

/// A handler that runs every tick
#[derive(Clone)]
pub struct TickHandler {
    /// Unique ID for this handler
    pub id: DefId,
    /// Condition for this handler to run
    pub condition: Option<Expr>,
    /// Target entities (by kind)
    pub target_kind: Option<DefId>,
    /// Effects to execute
    pub effects: Vec<Effect>,
    /// Priority (higher = runs first)
    pub priority: i32,
}

impl Runtime {
    /// Create a new runtime
    pub fn new() -> Self {
        Self {
            message_queue: VecDeque::new(),
            scheduled: Vec::new(),
            event_handlers: Vec::new(),
            tick_handlers: Vec::new(),
        }
    }

    /// Register an event handler
    pub fn on_event(&mut self, handler: EventHandler) {
        self.event_handlers.push(handler);
        self.event_handlers
            .sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// Register a tick handler
    pub fn on_tick(&mut self, handler: TickHandler) {
        self.tick_handlers.push(handler);
        self.tick_handlers
            .sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// Queue a message for processing
    pub fn send(&mut self, msg: Msg) {
        self.message_queue.push_back(msg);
    }

    /// Schedule a message for a future tick
    pub fn schedule(&mut self, msg: Msg, delay_ticks: u64, current_tick: u64) {
        let target_tick = current_tick + delay_ticks;
        self.scheduled.push((target_tick, msg));
        self.scheduled.sort_by_key(|(tick, _)| *tick);
    }

    /// Advance the game by one tick
    pub fn tick(&mut self, model: &mut Model) -> UpdateResult {
        // Advance time
        model.advance_tick();
        let current_tick = model.current_tick();

        // Move scheduled messages that are due to the queue
        let due: Vec<Msg> = self
            .scheduled
            .iter()
            .filter(|(tick, _)| *tick <= current_tick)
            .map(|(_, msg)| msg.clone())
            .collect();
        self.scheduled.retain(|(tick, _)| *tick > current_tick);

        for msg in due {
            self.message_queue.push_back(msg);
        }

        // Send tick message
        self.send(Msg::tick(current_tick));

        // Process all queued messages
        self.process_queue(model)
    }

    /// Process all messages in the queue
    pub fn process_queue(&mut self, model: &mut Model) -> UpdateResult {
        let mut result = UpdateResult::new();
        let mut cmds = Vec::new();

        while let Some(msg) = self.message_queue.pop_front() {
            let update = self.update(model, msg);
            cmds.push(update.cmd);
            result.emitted_messages.extend(update.emitted_messages);
            result.effect_result.merge(update.effect_result);
        }

        result.cmd = Cmd::batch(cmds);
        result
    }

    /// Process a single message
    pub fn update(&mut self, model: &mut Model, msg: Msg) -> UpdateResult {
        let mut result = UpdateResult::new();

        match msg.kind {
            MsgKind::Tick => {
                // Run tick handlers
                for handler in self.tick_handlers.clone() {
                    self.run_tick_handler(model, &handler, &msg, &mut result);
                }
            }
            MsgKind::Event | MsgKind::ScheduledEvent => {
                // Find and run matching event handlers
                if let Some(event_id) = &msg.event_id {
                    let handlers: Vec<_> = self
                        .event_handlers
                        .iter()
                        .filter(|h| &h.event_id == event_id)
                        .cloned()
                        .collect();

                    for handler in handlers {
                        self.run_event_handler(model, &handler, &msg, &mut result);
                    }
                }
            }
            MsgKind::Command => {
                // Player actions are also handled as events
                if let Some(action_id) = &msg.event_id {
                    let handlers: Vec<_> = self
                        .event_handlers
                        .iter()
                        .filter(|h| &h.event_id == action_id)
                        .cloned()
                        .collect();

                    for handler in handlers {
                        self.run_event_handler(model, &handler, &msg, &mut result);
                    }
                }
            }
            _ => {
                // Other message types can be handled by custom handlers
            }
        }

        result
    }

    /// Run a tick handler
    fn run_tick_handler(
        &mut self,
        model: &mut Model,
        handler: &TickHandler,
        msg: &Msg,
        result: &mut UpdateResult,
    ) {
        // If handler targets a specific entity kind, run for each
        if let Some(kind) = &handler.target_kind {
            let entity_ids: Vec<_> = model.entities.by_kind(kind).map(|e| e.id).collect();

            for entity_id in entity_ids {
                let entity = model.entities.get(entity_id);
                if entity.is_none() {
                    continue;
                }

                // Check condition
                if let Some(condition) = &handler.condition {
                    let mut ctx = EvalContext::new(
                        &model.entities,
                        &model.globals,
                        &msg.params,
                        &mut model.rng,
                    );
                    if let Some(entity) = model.entities.get(entity_id) {
                        ctx = ctx.with_target(entity);
                    }

                    match condition.eval(&mut ctx) {
                        Ok(v) if !v.is_truthy() => continue,
                        Err(_) => continue,
                        _ => {}
                    }
                }

                // Execute effects
                let target = EntityRef::Entity(entity_id);
                for effect in &handler.effects {
                    self.execute_effect(
                        model,
                        effect,
                        &target,
                        &msg.params,
                        &mut result.effect_result,
                    );
                }
            }
        } else {
            // No target kind - run once globally
            if let Some(condition) = &handler.condition {
                let mut ctx =
                    EvalContext::new(&model.entities, &model.globals, &msg.params, &mut model.rng);

                match condition.eval(&mut ctx) {
                    Ok(v) if !v.is_truthy() => return,
                    Err(_) => return,
                    _ => {}
                }
            }

            for effect in &handler.effects {
                self.execute_effect(
                    model,
                    effect,
                    &EntityRef::Global,
                    &msg.params,
                    &mut result.effect_result,
                );
            }
        }
    }

    /// Run an event handler
    fn run_event_handler(
        &mut self,
        model: &mut Model,
        handler: &EventHandler,
        msg: &Msg,
        result: &mut UpdateResult,
    ) {
        // Check condition
        if let Some(condition) = &handler.condition {
            let target_entity = model.entities.resolve(&msg.target);
            let mut ctx =
                EvalContext::new(&model.entities, &model.globals, &msg.params, &mut model.rng);
            if let Some(entity) = target_entity {
                ctx = ctx.with_target(entity);
            }

            match condition.eval(&mut ctx) {
                Ok(v) if !v.is_truthy() => return,
                Err(_) => return,
                _ => {}
            }
        }

        // Execute effects
        for effect in &handler.effects {
            self.execute_effect(
                model,
                effect,
                &msg.target,
                &msg.params,
                &mut result.effect_result,
            );
        }
    }

    /// Execute an effect
    #[allow(clippy::only_used_in_recursion)]
    fn execute_effect(
        &mut self,
        model: &mut Model,
        effect: &Effect,
        target: &EntityRef,
        params: &ValueMap,
        result: &mut EffectResult,
    ) {
        match effect {
            Effect::SetProperty { property, value } => {
                // Evaluate with target entity context
                let target_entity = model.entities.resolve(target);
                let mut ctx =
                    EvalContext::new(&model.entities, &model.globals, params, &mut model.rng);
                if let Some(entity) = target_entity {
                    ctx = ctx.with_target(entity);
                }
                let eval_result = value.eval(&mut ctx);

                if let (Ok(v), Some(entity)) = (eval_result, model.entities.resolve_mut(target)) {
                    entity.set(property.clone(), v);
                }
            }
            Effect::ModifyProperty {
                property,
                op,
                value,
            } => {
                // Evaluate with target entity context
                let target_entity = model.entities.resolve(target);
                let mut ctx =
                    EvalContext::new(&model.entities, &model.globals, params, &mut model.rng);
                if let Some(entity) = target_entity {
                    ctx = ctx.with_target(entity);
                }
                let eval_result = value.eval(&mut ctx);

                if let (Ok(v), Some(entity)) = (eval_result, model.entities.resolve_mut(target)) {
                    if let Some(operand) = v.as_float() {
                        let current = entity.get_number(property).unwrap_or(0.0);
                        let new_value = op.apply(current, operand);
                        entity.set(property.clone(), new_value);
                    }
                }
            }
            Effect::SetGlobal { property, value } => {
                let mut ctx =
                    EvalContext::new(&model.entities, &model.globals, params, &mut model.rng);
                if let Ok(v) = value.eval(&mut ctx) {
                    model.globals.insert(property.clone(), v);
                }
            }
            Effect::ModifyGlobal {
                property,
                op,
                value,
            } => {
                let mut ctx =
                    EvalContext::new(&model.entities, &model.globals, params, &mut model.rng);
                if let Ok(v) = value.eval(&mut ctx) {
                    if let Some(operand) = v.as_float() {
                        let current = model
                            .globals
                            .get(property)
                            .and_then(|v| v.as_float())
                            .unwrap_or(0.0);
                        let new_value = op.apply(current, operand);
                        model
                            .globals
                            .insert(property.clone(), Value::Float(new_value));
                    }
                }
            }
            Effect::AddFlag(flag) => {
                if let Some(entity) = model.entities.resolve_mut(target) {
                    entity.add_flag(flag.clone());
                }
            }
            Effect::RemoveFlag(flag) => {
                if let Some(entity) = model.entities.resolve_mut(target) {
                    entity.remove_flag(flag);
                }
            }
            Effect::SpawnEntity { kind, properties } => {
                let entity = model.entities.create(kind.clone());
                let entity_id = entity.id;

                // Set properties
                for (key, value_expr) in properties {
                    let mut ctx =
                        EvalContext::new(&model.entities, &model.globals, params, &mut model.rng);
                    if let Ok(v) = value_expr.eval(&mut ctx) {
                        if let Some(entity) = model.entities.get_mut(entity_id) {
                            entity.set(key.clone(), v);
                        }
                    }
                }

                result.spawned.push(entity_id);
            }
            Effect::DestroyTarget => {
                if let Some(id) = target.as_entity_id() {
                    model.entities.remove(id);
                    result.destroyed.push(id);
                }
            }
            Effect::DestroyEntity(entity_ref) => {
                if let Some(id) = entity_ref.as_entity_id() {
                    model.entities.remove(id);
                    result.destroyed.push(id);
                }
            }
            Effect::EmitEvent {
                event,
                target: event_target,
                params: event_params,
            } => {
                let mut evaluated_params = ValueMap::new();
                for (key, expr) in event_params {
                    let mut ctx =
                        EvalContext::new(&model.entities, &model.globals, params, &mut model.rng);
                    if let Ok(v) = expr.eval(&mut ctx) {
                        evaluated_params.insert(key.clone(), v);
                    }
                }
                result
                    .emitted_events
                    .push((event.clone(), event_target.clone(), evaluated_params));
            }
            Effect::ScheduleEvent {
                event,
                target: event_target,
                delay_ticks,
                params: event_params,
            } => {
                let mut ctx =
                    EvalContext::new(&model.entities, &model.globals, params, &mut model.rng);
                if let Ok(delay_val) = delay_ticks.eval(&mut ctx) {
                    if let Some(delay) = delay_val.as_int() {
                        let mut evaluated_params = ValueMap::new();
                        for (key, expr) in event_params {
                            let mut ctx = EvalContext::new(
                                &model.entities,
                                &model.globals,
                                params,
                                &mut model.rng,
                            );
                            if let Ok(v) = expr.eval(&mut ctx) {
                                evaluated_params.insert(key.clone(), v);
                            }
                        }
                        result.scheduled_events.push((
                            event.clone(),
                            event_target.clone(),
                            delay as u64,
                            evaluated_params,
                        ));
                    }
                }
            }
            Effect::If {
                condition,
                then_effects,
                else_effects,
            } => {
                let mut ctx =
                    EvalContext::new(&model.entities, &model.globals, params, &mut model.rng);
                let cond_result = condition.eval(&mut ctx);

                let effects = if cond_result.map(|v| v.is_truthy()).unwrap_or(false) {
                    then_effects
                } else {
                    else_effects
                };

                for eff in effects {
                    self.execute_effect(model, eff, target, params, result);
                }
            }
            Effect::Sequence(effects) => {
                for eff in effects {
                    self.execute_effect(model, eff, target, params, result);
                }
            }
            Effect::ForEachEntity {
                kind,
                filter,
                effects,
            } => {
                let entity_ids: Vec<_> = model.entities.by_kind(kind).map(|e| e.id).collect();

                for entity_id in entity_ids {
                    // Check filter
                    if let Some(filter_expr) = filter {
                        let entity = model.entities.get(entity_id);
                        let mut ctx = EvalContext::new(
                            &model.entities,
                            &model.globals,
                            params,
                            &mut model.rng,
                        );
                        if let Some(e) = entity {
                            ctx = ctx.with_target(e);
                        }

                        match filter_expr.eval(&mut ctx) {
                            Ok(v) if !v.is_truthy() => continue,
                            Err(_) => continue,
                            _ => {}
                        }
                    }

                    let entity_target = EntityRef::Entity(entity_id);
                    for eff in effects {
                        self.execute_effect(model, eff, &entity_target, params, result);
                    }
                }
            }
            Effect::RandomChoice { choices } => {
                let mut weights = Vec::new();
                for (weight_expr, _) in choices {
                    let mut ctx =
                        EvalContext::new(&model.entities, &model.globals, params, &mut model.rng);
                    let weight = weight_expr
                        .eval(&mut ctx)
                        .ok()
                        .and_then(|v| v.as_float())
                        .unwrap_or(0.0);
                    weights.push(weight);
                }

                if let Some(index) = model.rng.weighted_index(&weights) {
                    if let Some((_, effects)) = choices.get(index) {
                        for eff in effects {
                            self.execute_effect(model, eff, target, params, result);
                        }
                    }
                }
            }
            Effect::Log { level, message } => {
                let mut ctx =
                    EvalContext::new(&model.entities, &model.globals, params, &mut model.rng);
                if let Ok(v) = message.eval(&mut ctx) {
                    result.logs.push((*level, format!("{}", v)));
                }
            }
            Effect::Notify {
                kind,
                title,
                message,
                target: notify_target,
            } => {
                let mut ctx =
                    EvalContext::new(&model.entities, &model.globals, params, &mut model.rng);
                let title_str = title
                    .eval(&mut ctx)
                    .map(|v| format!("{}", v))
                    .unwrap_or_default();
                let msg_str = message
                    .eval(&mut ctx)
                    .map(|v| format!("{}", v))
                    .unwrap_or_default();

                result.notifications.push(crate::effect::Notification {
                    kind: kind.clone(),
                    title: title_str,
                    message: msg_str,
                    target: notify_target.clone(),
                });
            }
            _ => {
                // Handle remaining effect types
            }
        }
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::ModifyOp;

    #[test]
    fn test_runtime_tick() {
        let mut model = Model::new();
        let mut runtime = Runtime::new();

        // Add a tick handler that increments a global counter
        runtime.on_tick(TickHandler {
            id: DefId::new("counter"),
            condition: None,
            target_kind: None,
            effects: vec![Effect::ModifyGlobal {
                property: "tick_count".to_string(),
                op: ModifyOp::Add,
                value: Expr::lit(1.0),
            }],
            priority: 0,
        });

        // Initial state
        model.set_global("tick_count", 0.0f64);

        // Run a few ticks
        runtime.tick(&mut model);
        runtime.tick(&mut model);
        runtime.tick(&mut model);

        assert_eq!(
            model.get_global("tick_count").and_then(|v| v.as_float()),
            Some(3.0)
        );
        assert_eq!(model.current_tick(), 3);
    }

    #[test]
    fn test_runtime_event() {
        let mut model = Model::new();
        let mut runtime = Runtime::new();

        // Create an entity
        let entity = model.entities.create("nation");
        entity.set("gold", 100.0f64);
        let entity_id = entity.id;

        // Add event handler
        runtime.on_event(EventHandler {
            event_id: DefId::new("add_gold"),
            condition: None,
            effects: vec![Effect::ModifyProperty {
                property: "gold".to_string(),
                op: ModifyOp::Add,
                value: Expr::param("amount"),
            }],
            priority: 0,
        });

        // Send event
        let msg =
            Msg::event("add_gold", EntityRef::Entity(entity_id), 0).with_param("amount", 50.0f64);
        runtime.send(msg);
        runtime.process_queue(&mut model);

        assert_eq!(
            model
                .entities
                .get(entity_id)
                .and_then(|e| e.get_number("gold")),
            Some(150.0)
        );
    }
}

// ============================================================================
// Journal Integration (feature = "journal")
// ============================================================================

#[cfg(feature = "journal")]
use crate::journal::Journal;

#[cfg(feature = "journal")]
impl Runtime {
    /// Advance the game by one tick, recording to the journal
    pub fn tick_with_journal(&mut self, model: &mut Model, journal: &mut Journal) -> UpdateResult {
        // Advance time
        model.advance_tick();
        let current_tick = model.current_tick();

        // Record tick boundary
        journal.record_tick(current_tick);

        // Move scheduled messages that are due to the queue
        let due: Vec<Msg> = self
            .scheduled
            .iter()
            .filter(|(tick, _)| *tick <= current_tick)
            .map(|(_, msg)| msg.clone())
            .collect();
        self.scheduled.retain(|(tick, _)| *tick > current_tick);

        for msg in due {
            self.message_queue.push_back(msg);
        }

        // Send tick message
        self.send(Msg::tick(current_tick));

        // Process all queued messages with journal
        let result = self.process_queue_with_journal(model, journal);

        // Take snapshot if needed
        if journal.should_snapshot(current_tick) {
            journal.take_snapshot(model);
        }

        result
    }

    /// Process all messages in the queue, recording to the journal
    pub fn process_queue_with_journal(
        &mut self,
        model: &mut Model,
        journal: &mut Journal,
    ) -> UpdateResult {
        let mut result = UpdateResult::new();
        let mut cmds = Vec::new();
        let current_tick = model.current_tick();

        while let Some(msg) = self.message_queue.pop_front() {
            // Record the message before processing
            journal.record_message(current_tick, msg.clone());

            let update = self.update(model, msg);
            cmds.push(update.cmd);
            result.emitted_messages.extend(update.emitted_messages);
            result.effect_result.merge(update.effect_result);
        }

        result.cmd = Cmd::batch(cmds);
        result
    }

    /// Replay the journal to a specific tick
    ///
    /// This will:
    /// 1. Find the nearest snapshot before the target tick
    /// 2. Restore the model from that snapshot
    /// 3. Replay all messages from the snapshot to the target tick
    pub fn replay_to(&mut self, model: &mut Model, journal: &Journal, target_tick: u64) -> bool {
        // Find nearest snapshot
        let snapshot = journal.snapshot_at_or_before(target_tick);

        // Restore from snapshot or start fresh
        if let Some(snapshot) = snapshot {
            *model = snapshot.model.clone();
        } else {
            // No snapshot, need to replay from beginning
            *model = Model::new();
        }

        let start_tick = model.current_tick();

        // Replay messages from start_tick to target_tick
        let entries = journal.entries_in_range(start_tick, target_tick);
        for entry in entries {
            if let crate::journal::JournalEntry::Message { msg, .. } = entry {
                self.message_queue.push_back(msg.clone());
            }
        }

        // Process replayed messages
        self.process_queue(model);

        true
    }

    /// Step back one tick (if possible)
    ///
    /// Returns the tick we stepped back to, or None if at the beginning
    pub fn step_back(&mut self, model: &mut Model, journal: &Journal) -> Option<u64> {
        let current_tick = model.current_tick();
        if current_tick == 0 {
            return None;
        }

        let target_tick = current_tick - 1;
        self.replay_to(model, journal, target_tick);
        Some(target_tick)
    }

    /// Step forward one tick (replay)
    ///
    /// Returns the tick we stepped to
    pub fn step_forward(&mut self, model: &mut Model, journal: &Journal) -> u64 {
        let current_tick = model.current_tick();
        let target_tick = current_tick + 1;
        self.replay_to(model, journal, target_tick);
        target_tick
    }
}

#[cfg(all(test, feature = "journal"))]
mod journal_tests {
    use super::*;
    use crate::journal::{Journal, JournalConfig};

    #[test]
    fn test_tick_with_journal() {
        let mut model = Model::new();
        let mut runtime = Runtime::new();
        let mut journal = Journal::new();
        journal.start_recording();

        // Run a few ticks
        for _ in 0..5 {
            runtime.tick_with_journal(&mut model, &mut journal);
        }

        let stats = journal.stats();
        assert_eq!(stats.tick_count, 5);
        assert!(stats.message_count >= 5); // At least one Tick message per tick
    }

    #[test]
    fn test_replay_to() {
        let mut model = Model::new();
        let mut runtime = Runtime::new();
        let mut journal = Journal::with_config(JournalConfig {
            recording_enabled: true,
            snapshot_interval: 2, // Snapshot every 2 ticks
            ..Default::default()
        });

        // Run 10 ticks
        for _ in 0..10 {
            runtime.tick_with_journal(&mut model, &mut journal);
        }

        assert_eq!(model.current_tick(), 10);

        // Replay to tick 5
        runtime.replay_to(&mut model, &journal, 5);
        assert!(model.current_tick() <= 5);
    }

    #[test]
    fn test_step_back() {
        let mut model = Model::new();
        let mut runtime = Runtime::new();
        let mut journal = Journal::with_config(JournalConfig {
            recording_enabled: true,
            snapshot_interval: 1, // Snapshot every tick for easy stepping
            ..Default::default()
        });

        // Run 5 ticks
        for _ in 0..5 {
            runtime.tick_with_journal(&mut model, &mut journal);
        }

        let initial_tick = model.current_tick();

        // Step back
        let result = runtime.step_back(&mut model, &journal);
        assert!(result.is_some());
        assert!(model.current_tick() < initial_tick);
    }
}
