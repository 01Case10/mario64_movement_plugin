extends Node
## Headless verification: run the demo scene, drive input, check movement.
## Usage: godot --headless res://verify.tscn

func _ready() -> void:
	var scene: PackedScene = load("res://main.tscn")
	var main = scene.instantiate()
	get_tree().root.add_child.call_deferred(main)
	await get_tree().process_frame
	await get_tree().process_frame
	await get_tree().process_frame
	_run(main)
func _run(main: Node) -> void:
	main.input_enabled = false
	var player = main.get_node("Player")
	var world = main.get_node("World")
	print("VERIFY triangles=", world.triangle_count())
	assert(world.triangle_count() > 0, "no baked triangles")

	# Hold forward for 60 physics frames.
	for i in 60:
		player.set_stick(0.0, 1.0)
		player.set_buttons(0)
		await get_tree().physics_frame
	var a0: int = player.get_action()
	var p0: Vector3 = player.global_position
	print("VERIFY after walk: action=0x%X pos=%s speed=%.1f" % [a0, p0, player.get_forward_speed()])
	assert(a0 == 0x40000440, "expected Walking")
	assert(p0.z > -5.0, "expected forward motion (+z)")

	# Jump (A press).
	player.set_buttons(1)
	await get_tree().physics_frame
	player.set_buttons(0)
	await get_tree().physics_frame
	var a1: int = player.get_action()
	print("VERIFY after jump: action=0x%X" % a1)
	assert(a1 == 0x03000880, "expected Jump")

	# Let it land.
	for i in 120:
		await get_tree().physics_frame
	var a2: int = player.get_action()
	var p2: Vector3 = player.global_position
	print("VERIFY after land: action=0x%X pos=%s" % [a2, p2])
	assert(p2.y < 1.0, "expected to have landed")

	print("VERIFY ALL PASS")
	get_tree().quit(0)
