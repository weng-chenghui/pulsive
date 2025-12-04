extends Node
## Demo script showing pulsive-godot integration

@onready var engine: PulsiveEngine = $PulsiveEngine
@onready var info_label: Label = %Info
@onready var entities_label: Label = %Entities

var entity_count := 0

func _ready() -> void:
	# Initialize with in-memory database (no persistence)
	if engine.initialize_in_memory():
		print("Pulsive engine initialized!")
	else:
		push_error("Failed to initialize pulsive engine")
	
	update_ui()

func _on_tick_pressed() -> void:
	var result = engine.tick()
	print("Tick result: ", result)
	update_ui()

func _on_create_entity_pressed() -> void:
	entity_count += 1
	var entity_id = engine.create_entity("demo_entity")
	engine.set_property(entity_id, "name", "Entity %d" % entity_count)
	engine.set_property(entity_id, "health", 100)
	engine.set_property(entity_id, "created_at_tick", engine.get_tick())
	print("Created entity: ", entity_id)
	update_ui()

func update_ui() -> void:
	info_label.text = "Tick: %d | Date: %s" % [engine.get_tick(), engine.get_date_string()]
	
	# List all demo entities
	var entity_ids = engine.entities_by_kind("demo_entity")
	if entity_ids.size() == 0:
		entities_label.text = "Entities: (none)"
	else:
		var lines := PackedStringArray()
		for id in entity_ids:
			var name = engine.get_property(id, "name")
			var health = engine.get_property(id, "health")
			lines.append("  - %s (ID: %d, Health: %s)" % [name, id, health])
		entities_label.text = "Entities:\n" + "\n".join(lines)

