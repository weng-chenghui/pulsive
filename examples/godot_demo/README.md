# Pulsive Godot Demo

A minimal Godot 4.2+ project demonstrating pulsive-godot integration.

## Setup

### 1. Build the pulsive-godot library

From the workspace root:

```bash
cargo build -p pulsive-godot --release
```

### 2. Copy the library to the bin/ directory

**macOS:**
```bash
cp target/release/libpulsive_godot.dylib examples/godot_demo/bin/
```

**Linux:**
```bash
cp target/release/libpulsive_godot.so examples/godot_demo/bin/
```

**Windows:**
```bash
copy target\release\pulsive_godot.dll examples\godot_demo\bin\
```

### 3. Open in Godot

1. Open Godot 4.2+
2. Import this project (`examples/godot_demo/`)
3. Run the main scene

## What This Demo Shows

- Initializing PulsiveEngine with an in-memory database
- Advancing simulation ticks
- Creating entities dynamically
- Setting and getting entity properties
- Querying entities by kind

## Project Structure

```
godot_demo/
├── project.godot          # Godot project config
├── pulsive.gdextension    # GDExtension config (tells Godot where to find the library)
├── bin/                   # Compiled libraries go here
│   └── .gitkeep
├── main.tscn              # Main scene
├── main.gd                # Demo script
└── README.md
```

## PulsiveEngine API

The `PulsiveEngine` node exposes these methods to GDScript:

### Initialization
- `initialize() -> bool` - Initialize with configured paths
- `initialize_in_memory() -> bool` - Initialize with in-memory database

### Entities
- `create_entity(kind: String) -> int` - Create entity, returns ID
- `delete_entity(id: int) -> bool` - Delete entity
- `get_property(id: int, prop: String) -> Variant` - Get property
- `set_property(id: int, prop: String, value: Variant)` - Set property
- `get_entity(id: int) -> Dictionary` - Get all properties
- `entities_by_kind(kind: String) -> PackedInt64Array` - Query by kind

### Globals
- `get_global(prop: String) -> Variant` - Get global property
- `set_global(prop: String, value: Variant)` - Set global property

### Time
- `get_tick() -> int` - Current tick number
- `get_date_string() -> String` - Current simulation date
- `set_speed(speed: int)` - Set speed (0=Paused, 1-5=VerySlow to VeryFast)
- `get_speed() -> int` - Get current speed
- `is_paused() -> bool` - Check if paused
- `toggle_pause()` - Toggle pause state

### Simulation
- `tick() -> Dictionary` - Advance one tick, returns results
- `send_action(type: String, target_id: int, params: Dictionary) -> Dictionary` - Send command
- `emit_event(event_id: String, target_id: int, params: Dictionary) -> Dictionary` - Emit event

### Persistence
- `save() -> bool` - Save state to database
- `load() -> bool` - Load state from database

