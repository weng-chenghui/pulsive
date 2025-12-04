---
layout: default
title: Pulsive - Reactive Engine for Rust
---

# Pulsive

A reactive programming engine for Rust, inspired by the Elm architecture.

## 🤖 AI-Generated Project

> **This project was almost entirely implemented by an AI agent (Claude).**
>
> The motivation is to demonstrate how effectively an AI agent can use Rust as a tool to implement well-known software patterns (Elm architecture, reactive programming, entity-component systems) while maintaining performance and correctness.
>
> The human provided high-level requirements and guidance; the AI handled architecture decisions, code implementation, testing, and documentation.

---

## What is Pulsive?

Pulsive is a **data-driven reactive engine** that separates your application logic from hardcoded implementations. Instead of writing `if/else` chains for every game event or system rule, you define entities, events, and effects in configuration files—and Pulsive evaluates them at runtime.

## Use Cases

| Domain | Examples |
|--------|----------|
| **Games** | Strategy games, simulations, turn-based systems |
| **Distributed Systems** | Multi-actor coordination, event sourcing |
| **IoT/Automation** | Reactive rule engines, home automation |
| **Financial** | Trading simulations, economic modeling |

## Core Concepts

```
┌─────────────────────────────────────────────────────────┐
│                    Your Application                     │
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

### Key Types

| Type | Purpose |
|------|---------|
| `Actor` | Any principal that submits commands |
| `Command` | A validated action to process |
| `Entity` | Dynamic object with properties and flags |
| `Clock` | Tick-based simulation time |
| `Model` | Complete system state |
| `Msg` | Event or command in the queue |

## Quick Start

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
| [pulsive-core](https://github.com/nicweng/pulsive/tree/main/crates/pulsive-core) | Core types, expression engine, Elm-style runtime |
| [pulsive-db](https://github.com/nicweng/pulsive/tree/main/crates/pulsive-db) | Database layer using native_db |
| [pulsive-script](https://github.com/nicweng/pulsive/tree/main/crates/pulsive-script) | RON script loader and schema definitions |
| [pulsive-godot](https://github.com/nicweng/pulsive/tree/main/crates/pulsive-godot) | Godot 4 GDExtension bindings |

## Examples

| Example | What it demonstrates |
|---------|---------------------|
| **simple_economy** | Tick-based economy with resource production |
| **typing_game** | Console game with event-driven input handling |
| **http_server** | Load balancing, caching, rate limiting |
| **godot_demo** | Godot 4 GDExtension integration |

## Features Implemented by AI

- ✅ Elm-style Model-Update-Command architecture
- ✅ Dynamic entity system with properties and flags
- ✅ Expression engine for runtime evaluation
- ✅ Multi-actor support with deterministic simulation
- ✅ Tick-based time with configurable speed
- ✅ RON script loading for data-driven content
- ✅ native_db integration for persistence
- ✅ Godot 4 GDExtension bindings
- ✅ HTTP server example with load balancing
- ✅ Docker Compose integration tests
- ✅ GitHub Actions CI/CD pipelines

## What This Experiment Shows

1. **Rust is AI-friendly**: Strong type system catches errors early, making iterative AI development effective
2. **Patterns translate well**: Elm architecture, ECS concepts, and reactive patterns were implemented correctly
3. **Testing matters**: The AI wrote tests that caught real bugs during development
4. **Documentation is natural**: API documentation and README files were generated alongside code

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.

---

[View on GitHub](https://github.com/nicweng/pulsive) | [Report Issues](https://github.com/nicweng/pulsive/issues)
