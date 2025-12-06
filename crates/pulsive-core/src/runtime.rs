//! Elm-style runtime for the reactive engine

use crate::{
    effect::EffectResult,
    expr::EvalContext,
    write_set::{PendingWrite, WriteSet},
    Cmd, DefId, Effect, EntityRef, Expr, Model, Msg, MsgKind, ValueMap,
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

    /// Advance the simulation by one tick
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

    /// Execute an effect using deferred writes
    ///
    /// This method uses a two-phase approach for leaf effects:
    /// 1. Collect writes into a WriteSet via `collect_effect`
    /// 2. Apply the WriteSet atomically to the model
    ///
    /// For control flow effects (Sequence, If, ForEachEntity, RandomChoice),
    /// each child effect is executed immediately (collect + apply) before
    /// processing the next child. This preserves sequential semantics where
    /// later effects can see state changes from earlier effects.
    ///
    /// This maintains identical behavior to direct mutation while establishing
    /// the infrastructure for parallel execution in Phase 2.
    fn execute_effect(
        &mut self,
        model: &mut Model,
        effect: &Effect,
        target: &EntityRef,
        params: &ValueMap,
        result: &mut EffectResult,
    ) {
        // Control flow effects need special handling to preserve sequential semantics
        match effect {
            Effect::Sequence(effects) => {
                // Execute each child effect immediately, so later effects see earlier changes
                for eff in effects {
                    self.execute_effect(model, eff, target, params, result);
                }
            }
            Effect::If {
                condition,
                then_effects,
                else_effects,
            } => {
                // Evaluate condition against current model state
                let mut ctx =
                    EvalContext::new(&model.entities, &model.globals, params, &mut model.rng);
                let target_entity = model.entities.resolve(target);
                if let Some(entity) = target_entity {
                    ctx = ctx.with_target(entity);
                }
                let cond_result = condition.eval(&mut ctx);

                let effects = if cond_result.map(|v| v.is_truthy()).unwrap_or(false) {
                    then_effects
                } else {
                    else_effects
                };

                // Execute chosen branch effects sequentially
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
                    // Check filter against current model state
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

                    // Execute effects for this entity sequentially
                    let entity_target = EntityRef::Entity(entity_id);
                    for eff in effects {
                        self.execute_effect(model, eff, &entity_target, params, result);
                    }
                }
            }
            Effect::RandomChoice { choices } => {
                // Evaluate weights against current model state (with target entity context)
                let mut weights = Vec::new();
                for (weight_expr, _) in choices {
                    let target_entity = model.entities.resolve(target);
                    let mut ctx =
                        EvalContext::new(&model.entities, &model.globals, params, &mut model.rng);
                    if let Some(entity) = target_entity {
                        ctx = ctx.with_target(entity);
                    }
                    let weight = weight_expr
                        .eval(&mut ctx)
                        .ok()
                        .and_then(|v| v.as_float())
                        .unwrap_or(0.0);
                    weights.push(weight);
                }

                if let Some(index) = model.rng.weighted_index(&weights) {
                    if let Some((_, effects)) = choices.get(index) {
                        // Execute chosen effects sequentially
                        for eff in effects {
                            self.execute_effect(model, eff, target, params, result);
                        }
                    }
                }
            }
            // For leaf effects, use collect + apply
            _ => {
                // Phase 1: Collect writes
                let write_set = self.collect_effect(model, effect, target, params, result);

                // Phase 2: Apply writes atomically
                let write_result = write_set.apply(model);

                // Merge spawned/destroyed entity info into EffectResult
                result.spawned.extend(write_result.spawned);
                result.destroyed.extend(write_result.destroyed);
            }
        }
    }

    /// Collect writes from an effect into a WriteSet without mutating the model
    ///
    /// This is the deferred-write version of `execute_effect`. It evaluates expressions
    /// and collects the resulting writes, which can be applied atomically later.
    ///
    /// Note: The model is still passed mutably for RNG access during expression evaluation,
    /// but entity/global state is not modified - only the WriteSet is populated.
    #[allow(clippy::only_used_in_recursion)]
    fn collect_effect(
        &mut self,
        model: &mut Model,
        effect: &Effect,
        target: &EntityRef,
        params: &ValueMap,
        result: &mut EffectResult,
    ) -> WriteSet {
        let mut writes = WriteSet::new();

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

                if let (Ok(v), Some(entity_id)) = (eval_result, target.as_entity_id()) {
                    writes.push(PendingWrite::SetProperty {
                        entity_id,
                        key: property.clone(),
                        value: v,
                    });
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

                if let (Ok(v), Some(entity_id)) = (eval_result, target.as_entity_id()) {
                    if let Some(operand) = v.as_float() {
                        writes.push(PendingWrite::ModifyProperty {
                            entity_id,
                            key: property.clone(),
                            op: op.clone(),
                            value: operand,
                        });
                    }
                }
            }
            Effect::SetGlobal { property, value } => {
                let mut ctx =
                    EvalContext::new(&model.entities, &model.globals, params, &mut model.rng);
                if let Ok(v) = value.eval(&mut ctx) {
                    writes.push(PendingWrite::SetGlobal {
                        key: property.clone(),
                        value: v,
                    });
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
                        writes.push(PendingWrite::ModifyGlobal {
                            key: property.clone(),
                            op: op.clone(),
                            value: operand,
                        });
                    }
                }
            }
            Effect::AddFlag(flag) => {
                if let Some(entity_id) = target.as_entity_id() {
                    writes.push(PendingWrite::AddFlag {
                        entity_id,
                        flag: flag.clone(),
                    });
                }
            }
            Effect::RemoveFlag(flag) => {
                if let Some(entity_id) = target.as_entity_id() {
                    writes.push(PendingWrite::RemoveFlag {
                        entity_id,
                        flag: flag.clone(),
                    });
                }
            }
            Effect::SpawnEntity { kind, properties } => {
                // Evaluate all property expressions
                let mut evaluated_props = ValueMap::new();
                for (key, value_expr) in properties {
                    let mut ctx =
                        EvalContext::new(&model.entities, &model.globals, params, &mut model.rng);
                    if let Ok(v) = value_expr.eval(&mut ctx) {
                        evaluated_props.insert(key.clone(), v);
                    }
                }

                writes.push(PendingWrite::SpawnEntity {
                    kind: kind.clone(),
                    properties: evaluated_props,
                });
            }
            Effect::DestroyTarget => {
                if let Some(id) = target.as_entity_id() {
                    writes.push(PendingWrite::DestroyEntity { id });
                }
            }
            Effect::DestroyEntity(entity_ref) => {
                if let Some(id) = entity_ref.as_entity_id() {
                    writes.push(PendingWrite::DestroyEntity { id });
                }
            }
            Effect::EmitEvent {
                event,
                target: event_target,
                params: event_params,
            } => {
                // Event emission goes to EffectResult, not WriteSet
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
                // Scheduled events go to EffectResult, not WriteSet
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
                    let child_writes = self.collect_effect(model, eff, target, params, result);
                    writes.extend(child_writes);
                }
            }
            Effect::Sequence(effects) => {
                for eff in effects {
                    let child_writes = self.collect_effect(model, eff, target, params, result);
                    writes.extend(child_writes);
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
                        let child_writes =
                            self.collect_effect(model, eff, &entity_target, params, result);
                        writes.extend(child_writes);
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
                            let child_writes =
                                self.collect_effect(model, eff, target, params, result);
                            writes.extend(child_writes);
                        }
                    }
                }
            }
            Effect::Log { level, message } => {
                // Logs go to EffectResult, not WriteSet
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
                // Notifications go to EffectResult, not WriteSet
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

        writes
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

    /// Test that read-after-write within a Sequence sees the intermediate state.
    ///
    /// This test verifies that when a Sequence contains:
    /// 1. An effect that sets a property to a new value
    /// 2. A subsequent effect that reads that property
    ///
    /// The second effect should see the value set by the first effect,
    /// not the original value before the sequence started.
    ///
    /// Expected behavior (original direct-mutation):
    /// - gold starts at 50
    /// - SetProperty sets gold = 100
    /// - ModifyProperty reads gold (should see 100), adds it: gold = 100 + 100 = 200
    ///
    /// Bug behavior (if deferred writes don't account for intermediate state):
    /// - gold starts at 50
    /// - Collect SetProperty(gold = 100)
    /// - Collect ModifyProperty reads gold (sees 50!), records Add(50)
    /// - Apply: gold = 100, then gold = 100 + 50 = 150 (WRONG!)
    #[test]
    fn test_sequence_read_after_write() {
        let mut model = Model::new();
        let mut runtime = Runtime::new();

        // Create an entity with initial gold = 50
        let entity = model.entities.create("nation");
        entity.set("gold", 50.0f64);
        let entity_id = entity.id;

        // Register an event handler with a Sequence that:
        // 1. Sets gold to 100
        // 2. Reads gold and adds it to itself (should double: 100 + 100 = 200)
        runtime.on_event(EventHandler {
            event_id: DefId::new("test_sequence"),
            condition: None,
            effects: vec![Effect::Sequence(vec![
                // First: Set gold = 100
                Effect::SetProperty {
                    property: "gold".to_string(),
                    value: Expr::lit(100.0),
                },
                // Second: Add current gold value to gold (should read 100, result in 200)
                Effect::ModifyProperty {
                    property: "gold".to_string(),
                    op: ModifyOp::Add,
                    value: Expr::prop("gold"), // This should read 100, not 50
                },
            ])],
            priority: 0,
        });

        // Trigger the event
        let msg = Msg::event("test_sequence", EntityRef::Entity(entity_id), 0);
        runtime.send(msg);
        runtime.process_queue(&mut model);

        // Expected: gold = 200 (100 + 100)
        // Bug would give: gold = 150 (100 + 50)
        let final_gold = model
            .entities
            .get(entity_id)
            .and_then(|e| e.get_number("gold"));

        assert_eq!(
            final_gold,
            Some(200.0),
            "Sequence read-after-write failed: gold should be 200 (100 + 100), got {:?}. \
             This indicates the second effect read the original value (50) instead of \
             the intermediate value (100) set by the first effect.",
            final_gold
        );
    }

    /// Test that conditional branches evaluate against intermediate state.
    ///
    /// If an earlier effect sets a property, a subsequent If condition
    /// should see the updated value when deciding which branch to take.
    ///
    /// Expected: gold=50 -> set gold=100 -> If(gold > 75) should be TRUE -> add bonus
    /// Bug: If condition sees gold=50, evaluates FALSE, skips bonus
    #[test]
    fn test_conditional_on_modified_state() {
        let mut model = Model::new();
        let mut runtime = Runtime::new();

        let entity = model.entities.create("nation");
        entity.set("gold", 50.0f64);
        entity.set("bonus", 0.0f64);
        let entity_id = entity.id;

        runtime.on_event(EventHandler {
            event_id: DefId::new("test_conditional"),
            condition: None,
            effects: vec![Effect::Sequence(vec![
                // First: Set gold to 100
                Effect::SetProperty {
                    property: "gold".to_string(),
                    value: Expr::lit(100.0),
                },
                // Second: If gold > 75, add bonus (should be true since gold=100)
                Effect::If {
                    condition: Expr::Gt(Box::new(Expr::prop("gold")), Box::new(Expr::lit(75.0))),
                    then_effects: vec![Effect::SetProperty {
                        property: "bonus".to_string(),
                        value: Expr::lit(50.0),
                    }],
                    else_effects: vec![],
                },
            ])],
            priority: 0,
        });

        let msg = Msg::event("test_conditional", EntityRef::Entity(entity_id), 0);
        runtime.send(msg);
        runtime.process_queue(&mut model);

        let bonus = model
            .entities
            .get(entity_id)
            .and_then(|e| e.get_number("bonus"));

        assert_eq!(
            bonus,
            Some(50.0),
            "Conditional on modified state failed: bonus should be 50 (condition gold>75 should be true), got {:?}. \
             This indicates the If condition read the original gold value (50) instead of the modified value (100).",
            bonus
        );
    }

    /// Test that ForEachEntity filter sees intermediate state changes.
    ///
    /// If a property is modified, subsequent ForEachEntity filters should
    /// see the updated value when deciding which entities to process.
    ///
    /// Expected: Set active=true, then ForEachEntity with filter(active==true) should include entity
    /// Bug: Filter sees original active=false, skips the entity
    #[test]
    fn test_foreach_filter_on_modified_state() {
        let mut model = Model::new();
        let mut runtime = Runtime::new();

        let entity = model.entities.create("unit");
        entity.set("active", false);
        entity.set("processed", false);
        let entity_id = entity.id;

        runtime.on_event(EventHandler {
            event_id: DefId::new("test_foreach"),
            condition: None,
            effects: vec![Effect::Sequence(vec![
                // First: Activate the entity
                Effect::SetProperty {
                    property: "active".to_string(),
                    value: Expr::lit(true),
                },
                // Second: Process all active entities
                Effect::ForEachEntity {
                    kind: DefId::new("unit"),
                    filter: Some(Expr::Eq(
                        Box::new(Expr::prop("active")),
                        Box::new(Expr::lit(true)),
                    )),
                    effects: vec![Effect::SetProperty {
                        property: "processed".to_string(),
                        value: Expr::lit(true),
                    }],
                },
            ])],
            priority: 0,
        });

        let msg = Msg::event("test_foreach", EntityRef::Entity(entity_id), 0);
        runtime.send(msg);
        runtime.process_queue(&mut model);

        let processed = model
            .entities
            .get(entity_id)
            .and_then(|e| e.get("processed"))
            .and_then(|v| v.as_bool());

        assert_eq!(
            processed,
            Some(true),
            "ForEachEntity filter on modified state failed: entity should be processed. \
             This indicates the filter saw the original active=false instead of modified active=true."
        );
    }

    /// Test multiple modifications with intermediate property reads.
    ///
    /// When multiple effects modify related properties, later effects should
    /// see intermediate values.
    ///
    /// Expected: Set multiplier=2, then gold *= multiplier -> gold = 100 * 2 = 200
    /// Bug: Reads old multiplier (1), gold = 100 * 1 = 100
    #[test]
    fn test_multiple_modifications_intermediate_read() {
        let mut model = Model::new();
        let mut runtime = Runtime::new();

        let entity = model.entities.create("nation");
        entity.set("gold", 100.0f64);
        entity.set("multiplier", 1.0f64);
        let entity_id = entity.id;

        runtime.on_event(EventHandler {
            event_id: DefId::new("test_multi_modify"),
            condition: None,
            effects: vec![Effect::Sequence(vec![
                // First: Set multiplier to 2
                Effect::SetProperty {
                    property: "multiplier".to_string(),
                    value: Expr::lit(2.0),
                },
                // Second: Multiply gold by multiplier (should use new multiplier=2)
                Effect::ModifyProperty {
                    property: "gold".to_string(),
                    op: ModifyOp::Mul,
                    value: Expr::prop("multiplier"), // Should read 2.0, not 1.0
                },
            ])],
            priority: 0,
        });

        let msg = Msg::event("test_multi_modify", EntityRef::Entity(entity_id), 0);
        runtime.send(msg);
        runtime.process_queue(&mut model);

        let gold = model
            .entities
            .get(entity_id)
            .and_then(|e| e.get_number("gold"));

        assert_eq!(
            gold,
            Some(200.0),
            "Multiple modifications failed: gold should be 200 (100 * 2), got {:?}. \
             This indicates the multiply read the original multiplier (1) instead of modified (2).",
            gold
        );
    }

    /// Test nested sequences maintain proper state visibility.
    ///
    /// Changes in outer sequence should be visible to inner sequences.
    #[test]
    fn test_nested_sequences() {
        let mut model = Model::new();
        let mut runtime = Runtime::new();

        let entity = model.entities.create("nation");
        entity.set("x", 1.0f64);
        let entity_id = entity.id;

        runtime.on_event(EventHandler {
            event_id: DefId::new("test_nested"),
            condition: None,
            effects: vec![Effect::Sequence(vec![
                // Outer: Set x = 10
                Effect::SetProperty {
                    property: "x".to_string(),
                    value: Expr::lit(10.0),
                },
                // Inner sequence that reads x
                Effect::Sequence(vec![
                    // Should read x=10, add it: x = 10 + 10 = 20
                    Effect::ModifyProperty {
                        property: "x".to_string(),
                        op: ModifyOp::Add,
                        value: Expr::prop("x"),
                    },
                ]),
            ])],
            priority: 0,
        });

        let msg = Msg::event("test_nested", EntityRef::Entity(entity_id), 0);
        runtime.send(msg);
        runtime.process_queue(&mut model);

        let x = model
            .entities
            .get(entity_id)
            .and_then(|e| e.get_number("x"));

        assert_eq!(
            x,
            Some(20.0),
            "Nested sequences failed: x should be 20 (10 + 10), got {:?}. \
             Inner sequence didn't see the outer sequence's modification.",
            x
        );
    }

    /// Test RandomChoice weights based on modified state.
    ///
    /// If a property used in weight calculation is modified, the weights
    /// should reflect the new value.
    ///
    /// Setup: luck=0, then set luck=100, RandomChoice with weights based on luck
    /// Expected: With luck=100, first choice (weight=luck=100) should always win over second (weight=0)
    /// Bug: With luck=0 (original), both have weight 0 or 100, wrong choice made
    #[test]
    fn test_random_choice_modified_weights() {
        let mut model = Model::with_seed(42); // Deterministic RNG
        let mut runtime = Runtime::new();

        let entity = model.entities.create("nation");
        entity.set("luck", 0.0f64);
        entity.set("outcome", 0.0f64);
        let entity_id = entity.id;

        runtime.on_event(EventHandler {
            event_id: DefId::new("test_random"),
            condition: None,
            effects: vec![Effect::Sequence(vec![
                // First: Set luck to 100 (guarantees first choice wins)
                Effect::SetProperty {
                    property: "luck".to_string(),
                    value: Expr::lit(100.0),
                },
                // Second: Random choice weighted by luck
                // luck=100 means first choice has weight 100, second has weight 0
                Effect::RandomChoice {
                    choices: vec![
                        (
                            Expr::prop("luck"), // Weight = luck = should be 100
                            vec![Effect::SetProperty {
                                property: "outcome".to_string(),
                                value: Expr::lit(1.0), // Good outcome
                            }],
                        ),
                        (
                            Expr::Sub(Box::new(Expr::lit(100.0)), Box::new(Expr::prop("luck"))), // Weight = 100 - luck = should be 0
                            vec![Effect::SetProperty {
                                property: "outcome".to_string(),
                                value: Expr::lit(-1.0), // Bad outcome
                            }],
                        ),
                    ],
                },
            ])],
            priority: 0,
        });

        let msg = Msg::event("test_random", EntityRef::Entity(entity_id), 0);
        runtime.send(msg);
        runtime.process_queue(&mut model);

        let outcome = model
            .entities
            .get(entity_id)
            .and_then(|e| e.get_number("outcome"));

        assert_eq!(
            outcome,
            Some(1.0),
            "RandomChoice with modified weights failed: outcome should be 1.0 (good outcome). \
             With luck=100, first choice (weight=100) should always win over second (weight=0). Got {:?}. \
             This indicates weights were calculated using original luck=0.",
            outcome
        );
    }

    /// Test global property read-after-write in sequence.
    ///
    /// Same as entity property test but for globals.
    #[test]
    fn test_global_read_after_write() {
        let mut model = Model::new();
        let mut runtime = Runtime::new();

        model.set_global("counter", 10.0f64);

        runtime.on_event(EventHandler {
            event_id: DefId::new("test_global"),
            condition: None,
            effects: vec![Effect::Sequence(vec![
                // First: Set counter to 100
                Effect::SetGlobal {
                    property: "counter".to_string(),
                    value: Expr::lit(100.0),
                },
                // Second: Add counter to itself (should read 100, result in 200)
                Effect::ModifyGlobal {
                    property: "counter".to_string(),
                    op: ModifyOp::Add,
                    value: Expr::global("counter"), // Should read 100, not 10
                },
            ])],
            priority: 0,
        });

        let msg = Msg::event("test_global", EntityRef::Global, 0);
        runtime.send(msg);
        runtime.process_queue(&mut model);

        let counter = model.get_global("counter").and_then(|v| v.as_float());

        assert_eq!(
            counter,
            Some(200.0),
            "Global read-after-write failed: counter should be 200 (100 + 100), got {:?}. \
             This indicates the second effect read the original value (10) instead of modified (100).",
            counter
        );
    }

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
    /// Advance the simulation by one tick, recording to the journal
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
