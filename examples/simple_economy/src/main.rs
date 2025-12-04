//! Simple Economy Example
//!
//! Demonstrates pulsive with a basic economy simulation.
//! Two nations produce and consume gold, with events that can trigger.

use pulsive_core::{
    effect::{Effect, ModifyOp},
    runtime::{EventHandler, TickHandler},
    DefId, EntityRef, Expr, Model, Runtime,
};

fn main() {
    println!("=== Pulsive Simple Economy Example ===\n");

    // Create model and runtime
    let mut model = Model::with_seed(42);
    let mut runtime = Runtime::new();

    // Set start date
    model.time = pulsive_core::Clock::with_start_date(1444, 11, 11);

    // Create two nations
    let france = model.entities.create("nation");
    france.set("name", "France");
    france.set("gold", 100.0f64);
    france.set("income", 10.0f64);
    france.set("expenses", 8.0f64);
    let france_id = france.id;

    let england = model.entities.create("nation");
    england.set("name", "England");
    england.set("gold", 80.0f64);
    england.set("income", 12.0f64);
    england.set("expenses", 10.0f64);
    let england_id = england.id;

    println!("Created nations:");
    println!("  France (ID: {}): {} gold", france_id, 100.0);
    println!("  England (ID: {}): {} gold\n", england_id, 80.0);

    // Register tick handler: Update gold each tick
    runtime.on_tick(TickHandler {
        id: DefId::new("economy_tick"),
        condition: None,
        target_kind: Some(DefId::new("nation")),
        effects: vec![
            // gold += income - expenses
            Effect::ModifyProperty {
                property: "gold".to_string(),
                op: ModifyOp::Add,
                value: Expr::Sub(
                    Box::new(Expr::prop("income")),
                    Box::new(Expr::prop("expenses")),
                ),
            },
        ],
        priority: 0,
    });

    // Register event handler: Bonus gold event
    runtime.on_event(EventHandler {
        event_id: DefId::new("bonus_gold"),
        condition: None,
        effects: vec![
            Effect::ModifyProperty {
                property: "gold".to_string(),
                op: ModifyOp::Add,
                value: Expr::param("amount"),
            },
        ],
        priority: 0,
    });

    // Simulate 5 ticks
    println!("Running simulation for 5 ticks...\n");
    
    for _ in 0..5 {
        let _result = runtime.tick(&mut model);
        
        let date = model.time.current_date();
        let france = model.entities.get(france_id).unwrap();
        let england = model.entities.get(england_id).unwrap();
        
        println!(
            "Tick {} ({}): France: {:.1} gold, England: {:.1} gold",
            model.time.tick,
            date,
            france.get_number("gold").unwrap_or(0.0),
            england.get_number("gold").unwrap_or(0.0),
        );
    }

    println!("\nSending bonus_gold event to France...\n");
    
    // Send a bonus gold event
    let msg = pulsive_core::Msg::event(
        "bonus_gold",
        EntityRef::Entity(france_id),
        model.time.tick,
    ).with_param("amount", 50.0f64);
    
    runtime.send(msg);
    runtime.process_queue(&mut model);
    
    let france = model.entities.get(france_id).unwrap();
    println!(
        "After bonus: France now has {:.1} gold",
        france.get_number("gold").unwrap_or(0.0),
    );

    // Continue simulation
    println!("\nContinuing for 3 more ticks...\n");
    
    for _ in 0..3 {
        runtime.tick(&mut model);
        
        let date = model.time.current_date();
        let france = model.entities.get(france_id).unwrap();
        let england = model.entities.get(england_id).unwrap();
        
        println!(
            "Tick {} ({}): France: {:.1} gold, England: {:.1} gold",
            model.time.tick,
            date,
            france.get_number("gold").unwrap_or(0.0),
            england.get_number("gold").unwrap_or(0.0),
        );
    }

    println!("\n=== Simulation Complete ===");
}
