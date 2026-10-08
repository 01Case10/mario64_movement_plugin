extends Node3D
## Action showcase: choreographed sequence demonstrating all plugin actions.
## Teleports between segments for reliability. Records via --write-movie.

@onready var world: StepWorld3D = $World
@onready var player: StepChar3D = $Player
@onready var camera: Camera3D = $Camera3D
@onready var action_label: Label = $HUD/ActionLabel
@onready var segment_label: Label = $HUD/SegmentLabel
@onready var stepbot = $Player/StepBot

var frame := 0
var current_action := 0

const ACTION_NAMES := {
	0x0C400201: "Idle",
	0x40000440: "Walking",
	0x40000443: "TurningAround",
	0x40000445: "Decelerating",
	0x40000446: "Landing",
	0x03000880: "Single Jump",
	0x03000881: "Double Jump",
	0x01000882: "Triple Jump",
	0x01000883: "Backflip",
	0x01000887: "Side Flip",
	0x03000888: "Long Jump",
	0x0188088A: "Dive",
	0x018808B0: "Wall Kick",
	0x0800034B: "Ledge Grab",
	0x400008B1: "Air Knockback",
	0x40000447: "Butt Slide",
	0x4000088C: "Freefall",
	0x300022E2: "Water Plunge",
	0x40000E3: "Swimming",
	0x40000010: "Crouch",
	0x40000011: "Crawl",
}

# Segments: (start_frame, label, teleport_pos, camera_offset)
# Camera offsets are chosen for guaranteed line-of-sight (no walls between).
const SEGMENTS := [
	[0, "Idle", Vector3(0, 0.1, 0), Vector3(0, 5, 9)],
	[60, "Walking", Vector3(0, 0.1, 0), Vector3(0, 5, 9)],
	[150, "Jump Chain: Single -> Double -> Triple", Vector3(0, 0.1, 0), Vector3(0, 5, 9)],
	[280, "Backflip", Vector3(0, 0.1, 5), Vector3(0, 5, 9)],
	[340, "Side Flip", Vector3(0, 0.1, 5), Vector3(0, 5, 9)],
	[400, "Long Jump", Vector3(0, 0.1, 0), Vector3(0, 5, 9)],
	[460, "Dive", Vector3(0, 0.1, 0), Vector3(0, 5, 9)],
	[520, "Ground Pound", Vector3(0, 0.1, 0), Vector3(0, 5, 9)],
	[580, "Wall Kick", Vector3(0, 0.1, 10), Vector3(0, 6, -6)],  # behind+above, wall at z=15
	[660, "Ledge Grab", Vector3(12, 0.1, 5), Vector3(0, 3, -7)],  # front view, ledge at z=10
	[730, "Butt Slide", Vector3(-8, 0.1, -2), Vector3(0, 4, 8)],
	[800, "Crouch & Crawl", Vector3(0, 0.1, 0), Vector3(0, 5, 9)],
	[870, "Water Plunge & Swim", Vector3(-12, 0.1, -4), Vector3(0, 6, 8)],
	[980, "Done", Vector3(0, 0.1, 0), Vector3(0, 5, 9)],
]

var cam_offset := Vector3(0, 5, 9)

func _ready() -> void:
	player.world_node = world
	# Bake level (Godot meters).
	world.bake_box(Vector3(-30, -1, -30), Vector3(30, 0, 30), 0)  # ground
	world.bake_box(Vector3(-3, 0, 14.5), Vector3(3, 6, 15.5), 0)  # wall
	world.bake_box(Vector3(9, 2.25, 8.5), Vector3(15, 2.75, 11.5), 0)  # ledge
	world.bake_ramp(Vector3(-10, 0, -2), Vector3(1, 0, 0), 4.0, 1.5, 3.0, 1)  # slide
	# Pool: floor at -3, water at 0.
	world.bake_box(Vector3(-17, -4, -15), Vector3(-7, -3, -5), 0)
	world.bake_water(0.0, -17, -7, -15, -5)
	# Cut a hole in the ground for the pool (bake 4 ground pieces around it).
	# Actually the ground box covers it; we'll teleport into the pool area
	# which is below ground level. The pool floor catches the fall.
	player.bake()
	player.teleport(Vector3(0, 0.1, 0))
	player.action_changed.connect(_on_action_changed)
	segment_label.text = SEGMENTS[0][1]

func _on_action_changed(new_action: int, _old: int) -> void:
	current_action = new_action
	var n = ACTION_NAMES.get(new_action, "0x%08X" % new_action)
	action_label.text = n
	if stepbot:
		stepbot.action = new_action

func _physics_process(_delta: float) -> void:
	frame += 1
	# Update segment label and teleport.
	for i in range(SEGMENTS.size()):
		if frame == SEGMENTS[i][0]:
			segment_label.text = SEGMENTS[i][1]
			var tp: Vector3 = SEGMENTS[i][2]
			cam_offset = SEGMENTS[i][3]
			player.teleport(tp)
			# Snap camera to the new location.
			camera.global_position = tp + cam_offset
			# Clear buttons on teleport.
			player.set_stick(0, 0)
			player.set_buttons(0)
	
	var sx := 0.0
	var sy := 0.0
	var b := 0
	
	# Choreography by frame.
	if frame < 60:
		pass  # Idle
	elif frame < 150:
		sy = 1.0  # Walk forward
	elif frame < 280:
		# Jump chain: A at 150, 190, 230 (timed for landings).
		sy = 1.0
		if frame == 155 or frame == 195 or frame == 235:
			b = 1  # A
	elif frame < 340:
		# Backflip: Z + A from standstill.
		if frame == 285:
			b = 4 | 1  # Z + A
	elif frame < 400:
		# Sideflip: walk, then stick-back + A.
		if frame < 360:
			sy = 1.0
		elif frame == 365:
			sy = -1.0
			b = 1
	elif frame < 460:
		# Long jump: run fast, then A+Z.
		sy = 1.0
		if frame == 430:
			b = 1 | 4
	elif frame < 520:
		# Dive: run fast, then B.
		sy = 1.0
		if frame == 490:
			b = 2
	elif frame < 580:
		# Ground pound: jump, then Z in air.
		sy = 1.0
		if frame == 525:
			b = 1
		if frame == 545:
			b = 4
	elif frame < 660:
		# Wall kick: run at wall (z=15), jump, A at wall.
		sy = 1.0
		if frame == 600:
			b = 1  # jump
		if frame == 625:
			b = 1  # wall kick
	elif frame < 730:
		# Ledge grab: jump toward ledge.
		sy = 1.0
		if frame == 665:
			b = 1
	elif frame < 800:
		# Butt slide: on the ramp, will slide.
		sx = 1.0
	elif frame < 870:
		# Crouch (Z) then crawl (Z+stick).
		if frame < 835:
			b = 4
		else:
			b = 4
			sy = 0.5
	elif frame < 980:
		# Swim: walk into water, then B to stroke.
		if frame < 900:
			sy = -1.0  # walk toward pool (negative z)
		else:
			sy = 0.5
			if frame % 30 == 0:
				b = 2  # B stroke
	else:
		if frame == 1000:
			get_tree().quit()
	
	player.set_stick(sx, sy)
	player.set_buttons(b)
	
	# Camera: follow with per-segment offset (guaranteed line-of-sight).
	var p = player.global_position
	var desired = p + cam_offset
	camera.global_position = camera.global_position.lerp(desired, 0.2)
	camera.look_at(Vector3(p.x, p.y + 1, p.z))
