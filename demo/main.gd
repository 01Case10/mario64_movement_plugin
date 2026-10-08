extends Node3D
## StepKit demo: drives a StepChar3D from keyboard input, bakes a small
## level (floor, wall-kick wall, ledge, slide ramp), follows with a camera.

@onready var world: StepWorld3D = $World
@onready var player: StepChar3D = $Player
@onready var camera: Camera3D = $Camera3D
@onready var action_label: Label = $HUD/ActionLabel
@onready var help_label: Label = $HUD/HelpLabel

var input_enabled := true

const ACTION_NAMES := {
	0x0C400201: "Idle",
	0x40000440: "Walking",
	0x40000443: "TurningAround",
	0x40000445: "Decelerating",
	0x40000446: "Landing",
	0x03000880: "Jump",
	0x03000881: "DoubleJump",
	0x01000882: "TripleJump",
	0x01000883: "Backflip",
	0x01000887: "SideFlip",
	0x03000888: "LongJump",
	0x0188088A: "Dive",
	0x018808B0: "WallKick",
	0x0800034B: "LedgeGrab",
	0x400008B1: "AirKnockback",
	0x40000447: "ButtSlide",
}

func _ready() -> void:
	player.world_node = world
	# Bake the level (Godot meters; unit_scale converts to SM64 units).
	world.bake_box(Vector3(-20, -1, -20), Vector3(20, 0, 20), 0)  # floor
	world.bake_box(Vector3(-2, 0, 4), Vector3(2, 4, 5), 0)        # wall/ledge
	world.bake_ramp(Vector3(-8, 0, -2), Vector3(1, 0, 0), 4.0, 1.2, 3.0, 1)  # slide
	player.bake()
	player.teleport(Vector3(0, 0.1, -6))
	player.action_changed.connect(_on_action_changed)
	player.jumped.connect(func(vy): print("jumped vy=", vy))
	player.landed.connect(func(fs): print("landed fall=", fs))
	player.wall_hit.connect(func(): print("wall hit"))
	player.ledge_grabbed.connect(func(): print("LEDGE GRAB"))
	help_label.text = "WASD/arrows: move  SPACE: A(jump)  SHIFT: B(dive)  C: Z(ground pound)\nRun at the wall, jump, then A again to wall-kick. Jump at the ledge to grab it."

func _physics_process(_delta: float) -> void:
	if not input_enabled:
		return
	var sx := Input.get_axis("move_left", "move_right")
	var sy := Input.get_axis("move_back", "move_forward")
	player.set_stick(sx, sy)
	var b := 0
	if Input.is_action_pressed("jump"): b |= 1
	if Input.is_action_pressed("dive"): b |= 2
	if Input.is_action_pressed("pound"): b |= 4
	player.set_buttons(b)
	# Follow camera.
	var pp := player.global_position
	var target := pp + Vector3(0, 4.5, 9.0)
	camera.global_position = camera.global_position.lerp(target, 0.08)
	camera.look_at(pp + Vector3(0, 1.2, 0))

func _process(_delta: float) -> void:
	var a := player.get_action()
	var nm: String = ACTION_NAMES.get(a, "0x%X" % a)
	action_label.text = "%s  speed=%.1f" % [nm, player.get_forward_speed()]

func _on_action_changed(new_action: int, _old_action: int) -> void:
	print("action -> 0x%X" % new_action)
