---
layout: default
title: Pulsive - Reactive Engine for Rust
---

# Pulsive

A reactive programming engine for Rust, inspired by the Elm architecture.

<div class="ai-notice">
<strong>🤖 AI-Built Project</strong><br>
This project was almost entirely implemented by an AI agent (Claude) in a single conversation session. The motivation is to demonstrate how well AI can use Rust as a tool to implement well-known software patterns while maintaining performance and correctness.
</div>

## What is Pulsive?

Pulsive is a **data-driven reactive engine** that brings the predictability of Elm's architecture to Rust. Instead of hardcoding game logic or business rules, everything is defined dynamically through entities, events, and expressions.

## Use Cases

| Domain | Examples |
|--------|----------|
| **Games** | Strategy games, simulations, turn-based systems |
| **Distributed Systems** | Multi-actor coordination, event sourcing |
| **IoT/Automation** | Reactive rule engines, home automation |
| **Financial** | Trading simulations, economic modeling |

## Core Architecture

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

## Key Concepts

| Concept | Description |
|---------|-------------|
| `Actor` | Any principal that submits commands (user, player, service, bot) |
| `Command` | A validated action to process |
| `Clock` | Simulation time with tick-based progression |
| `Entity` | A dynamic object with properties and flags |
| `Msg` | An event or command in the message queue |
| `Model` | The complete system state |

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

## Crates

| Crate | Description |
|-------|-------------|
| **pulsive-core** | Core types, expression engine, and Elm-style runtime |
| **pulsive-db** | Database layer using native_db |
| **pulsive-script** | RON script loader and schema definitions |
| **pulsive-godot** | Godot 4 GDExtension bindings |

## Examples

The repository includes several examples demonstrating different use cases:

- **simple_economy** - Economy simulation with tick-based resource updates
- **typing_game** - Console typing game with event-driven scoring  
- **http_server** - HTTP server with load balancing, caching, rate limiting
- **godot_demo** - Godot 4 project showing GDExtension integration

## Features

- ✅ Elm-style Model-Update-Command architecture
- ✅ Dynamic entities with properties and flags
- ✅ Expression engine for conditions and effects
- ✅ Multi-actor support with deterministic simulation
- ✅ Tick-based time with configurable speed
- ✅ RON script loading for data-driven content
- ✅ Embedded database (native_db)
- ✅ Godot 4 GDExtension bindings
- ✅ GitHub Actions CI/CD

## About This Project

This project serves as an experiment in AI-assisted software development. The entire codebase—including:

- 4 library crates with ~3,000 lines of Rust
- 4 example applications
- Docker integration tests
- GitHub Actions workflows
- Documentation

—was generated by Claude (Anthropic's AI) working with a human developer in a collaborative coding session. The human provided high-level requirements and guidance, while the AI wrote the implementation.

### Goals of This Experiment

1. **Demonstrate AI capabilities**: Show that AI can produce production-quality Rust code
2. **Test pattern implementation**: Verify AI understanding of software patterns (Elm architecture, ECS-like systems)
3. **Evaluate correctness**: All code compiles and passes tests
4. **Assess maintainability**: Code follows Rust idioms and best practices

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.

---

<p class="footer">
View the source on <a href="https://github.com/weng-chenghui/pulsive">GitHub</a>
</p>

