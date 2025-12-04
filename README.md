# Pulsive

A reactive programming engine for Rust, inspired by the Elm architecture. Designed for data-driven simulations with dynamic content loaded from scripts.

> **🤖 AI-Generated Project**: This project was almost entirely implemented by an AI agent (Claude). The motivation is to demonstrate how effectively an AI can use Rust to implement well-known software patterns while maintaining performance and correctness. [Learn more →](https://weng-chenghui.github.io/pulsive/)

## Use Cases

- **Games**: Strategy games, simulations, turn-based systems
- **Distributed Systems**: Multi-actor coordination, event sourcing
- **IoT/Automation**: Reactive rule engines, home automation
- **Financial**: Trading simulations, economic modeling

## Features

- **Elm-style Architecture**: Model-Update-Command pattern for predictable state management
- **Dynamic Data**: All entities, events, and properties are data-driven (not hardcoded)
- **Expression Engine**: Conditions and effects defined in RON scripts, evaluated at runtime
- **Embedded Database**: High-performance storage using native_db
- **Multi-Actor Ready**: Deterministic simulation with tick-based time and seeded RNG
- **Godot Integration**: GDExtension bindings for Godot 4

## Core Concepts

| Concept | Description |
|---------|-------------|
| `Actor` | Any principal that submits commands (user, player, service, bot) |
| `Command` | A validated action to process |
| `Context` | Session/state for an actor |
| `Clock` | Simulation time with tick-based progression |
| `Speed` | Processing rate control |
| `Timestamp` | Human-readable date representation |
| `Entity` | A dynamic object with properties and flags |
| `Msg` | An event or command in the message queue |
| `Model` | The complete system state |

## Crates

| Crate | Description |
|-------|-------------|
| `pulsive-core` | Core types, expression engine, and Elm-style runtime |
| `pulsive-db` | Database layer using native_db |
| `pulsive-script` | RON script loader and schema definitions |
| `pulsive-godot` | Godot 4 GDExtension bindings |

## Quick Start

```toml
[dependencies]
pulsive-core = "0.1"
```

```rust
use pulsive_core::{Model, Runtime, Msg, EntityRef};

fn main() {
    let mut model = Model::new();
    let mut runtime = Runtime::new();
    
    // Create an entity
    let entity = model.entities.create("sensor");
    entity.set("temperature", 22.5);
    entity.set("location", "room_1");
    
    // Advance simulation
    runtime.tick(&mut model);
    
    // Send an event
    let msg = Msg::event("temperature_alert", EntityRef::Global, model.current_tick())
        .with_param("threshold", 30.0);
    runtime.send(msg);
    runtime.process_queue(&mut model);
}
```

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Your Application                     │
│              (Godot, CLI, Web Server, etc.)             │
└─────────────────────────┬───────────────────────────────┘
                          │
┌─────────────────────────▼───────────────────────────────┐
│                    pulsive-core                         │
│   Model │ Runtime │ Entities │ Events │ Expressions    │
└─────────────────────────┬───────────────────────────────┘
                          │
┌─────────────────────────▼───────────────────────────────┐
│                    pulsive-db                           │
│            Persistence (native_db wrapper)              │
└─────────────────────────┬───────────────────────────────┘
                          │
┌─────────────────────────▼───────────────────────────────┐
│                   pulsive-script                        │
│           RON loader + schema definitions               │
└─────────────────────────────────────────────────────────┘
```

## Examples

| Example | Description |
|---------|-------------|
| `simple_economy` | Economy simulation with tick-based resource updates |
| `typing_game` | Console typing game with event-driven scoring |
| `http_server` | HTTP server with load balancing, caching, rate limiting |
| `godot_demo` | Godot 4 project showing GDExtension integration |

Run an example:
```bash
cargo run --example simple_economy
cargo run --example typing_game
cargo run --example http_server
```

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
