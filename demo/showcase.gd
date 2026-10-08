extends Node3D
## Moveset showcase for the stepkit plugin (Phases A-F).
##
## The camera rides a SpringArm3D rig: the rig smoothly follows the player and
## the spring arm shortens automatically when world geometry blocks the view,
## so walls never hide the character.
##
## Timing: the project runs physics at 30 Hz and recording uses --fixed-fps 30,
## so 1 script tick = 1 sim tick = 1 video frame.
##
## Record:
##   Xvfb :99 &
##   DISPLAY=:99 ~/workspace/build/Godot_v4.7.2-stable_linux.x86_64 --path demo \
##       --write-movie /tmp/showcase.avi --fixed-fps 30 res://showcase.tscn
## Convert:
##   ffmpeg -y -i /tmp/showcase.avi -pix_fmt yuv420p <out>.mp4

const BTN_A := 1
const BTN_B := 2
const BTN_Z := 4

# ---------------------------------------------------------------------------
# Action IDs — from crates/stepkit-core/src/state.rs `impl ActionId`
# (water states from crates/stepkit-core/src/actions/water.rs `id`).
# ---------------------------------------------------------------------------
const A_IDLE := 0x0C400201
const A_WALKING := 0x04000440
const A_TURNING_AROUND := 0x00000443
const A_FINISH_TURNING_AROUND := 0x00000444
const A_BRAKING := 0x04000445
const A_DECELERATING := 0x0400044A
const A_JUMP_LAND := 0x04000470
const A_FREEFALL_LAND := 0x04000471
const A_DOUBLE_JUMP_LAND := 0x04000472
const A_SIDE_FLIP_LAND := 0x04000473
const A_TRIPLE_JUMP_LAND := 0x04000478
const A_LONG_JUMP_LAND := 0x00000479
const A_BACKFLIP_LAND := 0x0400047A
const A_QUICKSAND_JUMP_LAND := 0x04000476
const A_JUMP := 0x03000880
const A_DOUBLE_JUMP := 0x03000881
const A_TRIPLE_JUMP := 0x01000882
const A_BACKFLIP := 0x01000883
const A_SIDE_FLIP := 0x01000887
const A_LONG_JUMP := 0x03000888
const A_DIVE := 0x0188088A
const A_SLIDE_KICK := 0x018008AA
const A_FORWARD_ROLLOUT := 0x010008A6
const A_BACKWARD_ROLLOUT := 0x010008AD
const A_FREEFALL := 0x4000088C
const A_GROUND_POUND := 0x400008A8
const A_WALL_KICK := 0x018808B0
const A_WALL_KICK_AIR := 0x03000886
const A_AIR_HIT_WALL := 0x000008A7
const A_STEEP_JUMP := 0x03000885
const A_SOFT_BONK := 0x010208B6
const A_BACKWARD_AIR_KB := 0x010208B0
const A_FORWARD_AIR_KB := 0x010208B1
const A_HARD_BACKWARD_AIR_KB := 0x010208B2
const A_HARD_FORWARD_AIR_KB := 0x010208B3
const A_BUTT_SLIDE := 0x00840452
const A_STOMACH_SLIDE := 0x008C0453
const A_DIVE_SLIDE := 0x00880456
const A_CROUCH_SLIDE := 0x04808459
const A_SLIDE_KICK_SLIDE := 0x0080045A
const A_LEDGE_GRAB := 0x0800034B
const A_LEDGE_CLIMB_FAST := 0x0000054F
const A_LEDGE_CLIMB_SLOW_1 := 0x0000054C
const A_LEDGE_CLIMB_SLOW_2 := 0x0000054D
const A_PUNCHING := 0x00800380
const A_MOVE_PUNCHING := 0x00800457
const A_JUMP_KICK := 0x018008AC
const A_BACKWARD_GROUND_KB := 0x00020462
const A_FORWARD_GROUND_KB := 0x00020463
const A_HARD_BACKWARD_GROUND_KB := 0x00020460
const A_HARD_FORWARD_GROUND_KB := 0x00020461
const A_SOFT_BACKWARD_GROUND_KB := 0x00020464
const A_SOFT_FORWARD_GROUND_KB := 0x00020465
const A_GROUND_BONK := 0x00020466
const A_CROUCH := 0x40000010
const A_CRAWL := 0x40000011
const A_IN_QUICKSAND := 0x0002020D
const A_WATER_JUMP := 0x01000889
const A_WATER_PLUNGE := 0x300022E2
const A_WATER_IDLE := 0x380022C0
const A_WATER_ACTION_END := 0x300022C2
const A_BREASTSTROKE := 0x300024D0
const A_SWIMMING_END := 0x300024D1
const A_FLUTTER_KICK := 0x300024D2

const ACTION_NAMES := {
	0x0C400201: "Idle",
	0x04000440: "Walking",
	0x00000443: "Turning Around",
	0x00000444: "Finish Turning",
	0x04000445: "Braking",
	0x0400044A: "Decelerating",
	0x04000470: "Jump Land",
	0x04000471: "Freefall Land",
	0x04000472: "Double Jump Land",
	0x04000473: "Side Flip Land",
	0x04000478: "Triple Jump Land",
	0x00000479: "Long Jump Land",
	0x0400047A: "Backflip Land",
	0x04000476: "Quicksand Jump Land",
	0x03000880: "Jump",
	0x03000881: "Double Jump",
	0x01000882: "Triple Jump",
	0x01000883: "Backflip",
	0x01000887: "Side Flip",
	0x03000888: "Long Jump",
	0x0188088A: "Dive",
	0x018008AA: "Slide Kick",
	0x010008A6: "Forward Rollout",
	0x010008AD: "Backward Rollout",
	0x4000088C: "Freefall",
	0x400008A8: "Ground Pound",
	0x018808B0: "Wall Kick",
	0x03000886: "Wall Kick (air)",
	0x000008A7: "Air Hit Wall",
	0x03000885: "Steep Jump",
	0x010208B6: "Soft Bonk",
	0x010208B0: "Backward Air Knockback",
	0x010208B1: "Forward Air Knockback",
	0x010208B2: "Hard Backward Air KB",
	0x010208B3: "Hard Forward Air KB",
	0x00840452: "Butt Slide",
	0x008C0453: "Stomach Slide",
	0x00880456: "Dive Slide",
	0x04808459: "Crouch Slide",
	0x0080045A: "Slide Kick Slide",
	0x0800034B: "Ledge Grab",
	0x0000054F: "Ledge Climb (fast)",
	0x0000054C: "Ledge Climb (slow 1)",
	0x0000054D: "Ledge Climb (slow 2)",
	0x00800380: "Punching",
	0x00800457: "Move Punching",
	0x018008AC: "Jump Kick",
	0x00020462: "Backward Ground KB",
	0x00020463: "Forward Ground KB",
	0x00020460: "Hard Backward Ground KB",
	0x00020461: "Hard Forward Ground KB",
	0x00020464: "Soft Backward Ground KB",
	0x00020465: "Soft Forward Ground KB",
	0x00020466: "Ground Bonk",
	0x40000010: "Crouch",
	0x40000011: "Crawl",
	0x0002020D: "In Quicksand",
	0x01000889: "Water Jump",
	0x300022E2: "Water Plunge",
	0x380022C0: "Water Idle (tread)",
	0x300022C2: "Water Action End",
	0x300024D0: "Breaststroke",
	0x300024D1: "Swimming End",
	0x300024D2: "Flutter Kick",
}

# Landing actions that chain into the next jump with A.
const CHAIN_LANDS := [0x04000470, 0x04000472, 0x04000471, 0x04000473]
# Water actions in which A near the surface triggers the water jump.
const SWIM_STATES := [0x380022C0, 0x300022C2, 0x300024D0, 0x300024D1, 0x300024D2]

# [start_tick, label, teleport_pos, rig_yaw_deg, spring_pitch_deg, spring_len, do_teleport]
# yaw 0 = camera on the +Z side of the player; yaw 180 = camera behind (-Z).
var SEGMENTS := [
	[0, "Idle", Vector3(0, 0.1, 0), 0, 28, 9.0, true],
	[30, "Walking", Vector3(0, 0.1, 0), 0, 28, 9.0, true],
	[90, "Jump Chain: Single - Double - Triple", Vector3(0, 0.1, 0), 0, 28, 9.0, true],
	[210, "Backflip (Z, then A)", Vector3(0, 0.1, 0), 0, 28, 9.0, true],
	[250, "Side Flip (run, pull back, A)", Vector3(0, 0.1, 0), 0, 28, 9.0, true],
	[305, "Crouch Slide - Long Jump (Z, then A)", Vector3(0, 0.1, 5), 0, 25, 9.0, true],
	[395, "Dive - Dive Slide - Rollout (B, then A)", Vector3(0, 0.1, 5), 0, 25, 9.0, true],
	[480, "Slide Kick (slide, then B)", Vector3(0, 0.1, 5), 0, 25, 9.0, true],
	[550, "Punch Combo: Jab, Jab, Kick", Vector3(0, 0.1, 0), 0, 28, 9.0, true],
	[615, "Jump Kick (jump, then B)", Vector3(0, 0.1, 0), 0, 28, 9.0, true],
	[670, "Ground Pound (jump, then Z)", Vector3(0, 0.1, 0), 0, 30, 9.0, true],
	[750, "Ground Bonk (dive slide into wall)", Vector3(0, 0.1, 0), 180, 25, 9.0, true],
	[820, "Wall Kick (jump at wall, A)", Vector3(0, 0.1, 10), 180, 30, 9.0, true],
	[895, "Ledge Grab - Fast Climb (A)", Vector3(12, 0.1, 5), 180, 20, 8.0, true],
	[985, "Butt Slide (slippery ramp)", Vector3(-9, 0.5, -2), 90, 25, 9.0, true],
	[1045, "Crouch & Crawl", Vector3(0, 0.1, 0), 0, 28, 9.0, true],
	[1105, "Quicksand (sinks; weak jump)", Vector3(8, 0.1, -8), 0, 28, 9.0, true],
	[1165, "Water: Plunge - Tread", Vector3(-8.5, -1.5, -10), 0, 35, 7.0, true],
	[1285, "Breaststroke Chain (tap A)", Vector3.ZERO, 0, 35, 7.0, false],
	[1395, "Flutter Kick (hold A) - Water Jump", Vector3(-8.5, -1.0, -10), 0, 35, 7.0, true],
	[1545, "Done", Vector3(0, 0.1, 0), 0, 28, 9.0, true],
]
const QUIT_TICK := 1570

@onready var world: StepWorld3D = $World
@onready var player: StepChar3D = $Player
@onready var rig: Node3D = $CameraRig
@onready var spring: SpringArm3D = $CameraRig/SpringArm3D
@onready var camera: Camera3D = $CameraRig/SpringArm3D/Camera3D
@onready var action_label: Label = $HUD/ActionLabel
@onready var segment_label: Label = $HUD/SegmentLabel
@onready var stepbot = $Player/StepBot

var tick := 0
var current_action := A_IDLE
var seg_idx := -1
var seg_tick := 0
var in_sx := 0.0
var in_sy := 0.0
var in_b := 0
var press_bits := 0
var press_left := 0
var last_press_tick := -100
var aux_flag := false
var aux_tick := -1


func _ready() -> void:
	player.world_node = world
	world.bake_box(Vector3(-30, -1, -30), Vector3(-17, 0, 30), 0)   # ground west of pool
	world.bake_box(Vector3(-7, -1, -30), Vector3(30, 0, 30), 0)     # ground east of pool
	world.bake_box(Vector3(-17, -1, -30), Vector3(-7, 0, -15), 0)   # ground north of pool
	world.bake_box(Vector3(-17, -1, -5), Vector3(-7, 0, 30), 0)     # ground south of pool
	world.bake_box(Vector3(-3, 0, 14.5), Vector3(3, 6, 15.5), 0)    # wall (z=15)
	world.bake_box(Vector3(9, 2.25, 8.5), Vector3(15, 2.75, 11.5), 0) # ledge
	world.bake_ramp(Vector3(-10, 0, -2), Vector3(1, 0, 0), 4.0, 1.5, 3.0, 1) # slide ramp (rises +x)
	world.bake_box(Vector3(6, -0.2, -10), Vector3(10, 0.0, -6), 2)  # quicksand patch
	# Pool: floor + containment walls + water (plunge needs 100+ units of depth).
	world.bake_box(Vector3(-17, -4, -15), Vector3(-7, -3, -5), 0)   # pool floor
	world.bake_box(Vector3(-17.5, -4, -15.5), Vector3(-17, 0, -4.5), 0)
	world.bake_box(Vector3(-7, -4, -15.5), Vector3(-6.5, 0, -4.5), 0)
	world.bake_box(Vector3(-17.5, -4, -15.5), Vector3(-6.5, 0, -15), 0)
	world.bake_box(Vector3(-17.5, -4, -5.5), Vector3(-6.5, 0, -5), 0)
	world.bake_water(0.0, -17, -7, -15, -5)
	player.bake()
	player.teleport(Vector3(0, 0.1, 0))
	rig.global_position = player.global_position + Vector3(0, 1.5, 0)
	player.action_changed.connect(_on_action_changed)


func _on_action_changed(new_action: int, _old_action: int) -> void:
	current_action = new_action
	action_label.text = "Action: " + str(ACTION_NAMES.get(new_action, "0x%08X" % new_action))
	stepbot.action = new_action


func _press(bits: int, ticks: int) -> void:
	press_bits = bits
	press_left = ticks


func _enter_segment(i: int) -> void:
	seg_idx = i
	seg_tick = 0
	press_bits = 0
	press_left = 0
	last_press_tick = -100
	aux_flag = false
	aux_tick = -1
	var s = SEGMENTS[i]
	segment_label.text = s[1]
	if s[6]:
		player.teleport(s[2])
		player.set_stick(0, 0)
		player.set_buttons(0)
		rig.global_position = player.global_position + Vector3(0, 1.5, 0)
	rig.rotation.y = deg_to_rad(float(s[3]))
	spring.rotation.x = -deg_to_rad(float(s[4]))
	spring.spring_length = float(s[5])


func _physics_process(_delta: float) -> void:
	for i in range(SEGMENTS.size()):
		if tick == int(SEGMENTS[i][0]):
			_enter_segment(i)
	in_sx = 0.0
	in_sy = 0.0
	in_b = 0
	_choreography()
	if press_left > 0:
		in_b |= press_bits
		press_left -= 1
	player.set_stick(in_sx, in_sy)
	player.set_buttons(in_b)
	# Rig smoothly follows the player at head height; the spring arm shortens
	# on collision with the StaticBody3D colliders so walls never hide the player.
	var target := player.global_position + Vector3(0, 1.5, 0)
	rig.global_position = rig.global_position.lerp(target, 0.3)
	seg_tick += 1
	tick += 1
	if tick >= QUIT_TICK:
		get_tree().quit()


func _process(_delta: float) -> void:
	# Keep the player framed even as the spring arm shortens on occlusion.
	if is_instance_valid(player) and is_instance_valid(camera):
		var aim := player.global_position + Vector3(0, 1.0, 0)
		if camera.global_position.distance_squared_to(aim) > 0.0001:
			camera.look_at(aim)


func _choreography() -> void:
	match seg_idx:
		1:  # Walking
			in_sy = 1.0
		2:  # Jump chain: press A on every chainable landing.
			in_sy = 1.0
			if seg_tick == 2:
				_press(BTN_A, 2)
			elif current_action in CHAIN_LANDS and tick - last_press_tick > 12:
				_press(BTN_A, 2)
				last_press_tick = tick
		3:  # Backflip: Z to crouch, then A while crouching.
			if seg_tick >= 5 and seg_tick < 8:
				in_b = BTN_Z
			elif seg_tick >= 8 and seg_tick < 10:
				in_b = BTN_Z | BTN_A
		4:  # Side flip: run, pull stick back (>90 deg), A during the turn.
			if seg_tick < 35:
				in_sy = 1.0
			else:
				in_sy = -1.0
			if seg_tick == 42:
				_press(BTN_A, 2)
		5:  # Crouch slide -> long jump: run, Z, then A within 30 frames.
			in_sy = -1.0
			if seg_tick == 40:
				_press(BTN_Z, 2)
			elif seg_tick == 48:
				_press(BTN_A, 2)
		6:  # Dive -> dive slide -> rollout: run, B, A during the slide.
			in_sy = -1.0
			if seg_tick == 48:
				_press(BTN_B, 2)
			elif current_action == A_DIVE_SLIDE and not aux_flag:
				_press(BTN_A, 2)
				aux_flag = true
		7:  # Slide kick: run, Z, B during the crouch slide.
			in_sy = -1.0
			if seg_tick == 35:
				_press(BTN_Z, 2)
			elif seg_tick == 43:
				_press(BTN_B, 2)
		8:  # Punch combo: B chains jab -> jab -> kick.
			if seg_tick == 5 or seg_tick == 11 or seg_tick == 19:
				_press(BTN_B, 2)
		9:  # Jump kick: jump, B while airborne and slow.
			if seg_tick == 5:
				_press(BTN_A, 6)
			elif current_action == A_JUMP and seg_tick > 10 and not aux_flag:
				_press(BTN_B, 2)
				aux_flag = true
		10:  # Ground pound: jump, Z in the air.
			if seg_tick == 5:
				_press(BTN_A, 8)
			elif seg_tick == 22:
				_press(BTN_Z, 2)
		11:  # Ground bonk: dive slide into the wall at speed.
			in_sy = 1.0
			if seg_tick == 40:
				_press(BTN_B, 2)
		12:  # Wall kick: jump at the wall, A on wall contact.
			in_sy = 1.0
			if seg_tick == 8:
				_press(BTN_A, 8)
			elif current_action == A_AIR_HIT_WALL and not aux_flag:
				_press(BTN_A, 4)
				aux_flag = true
		13:  # Ledge grab -> fast climb: run into the ledge wall, jump, A to climb.
			if seg_tick < 15:
				in_sy = 1.0
			if seg_tick == 18:
				_press(BTN_A, 6)
			if current_action == A_LEDGE_GRAB and aux_tick < 0:
				aux_tick = tick
			if aux_tick >= 0 and tick - aux_tick > 20:
				_press(BTN_A, 2)
				aux_tick = -2
		14:  # Butt slide: walk downhill on the slippery ramp.
			in_sx = -1.0 if seg_tick < 10 else 0.0
		15:  # Crouch & crawl: hold Z, then Z + stick.
			if seg_tick < 30:
				in_b = BTN_Z
			else:
				in_b = BTN_Z
				in_sy = 0.5
		16:  # Quicksand: stand and sink, then jump out (weakened).
			if seg_tick == 45:
				_press(BTN_A, 2)
		17:  # Water: fall in from height -> plunge -> tread. (no input needed)
			pass
		18:  # Breaststroke: tap A for stroke chains. Weave to stay in the pool,
			# stay deep so A taps don't trigger water jumps. (0.3 weave keeps
			# the visual body off the pool walls; the sim has no water walls.)
			in_sx = 0.3 * sin(seg_tick * 0.15)
			in_sy = 0.0
			if seg_tick % 30 == 5 and player.global_position.y < -0.3:
				_press(BTN_A, 2)
		19:  # Flutter kick, rise, then A + hard stick at the surface -> water jump.
			if seg_tick < 90:
				in_b = BTN_A
				in_sx = 0.6 * sin(seg_tick * 0.15)
			else:
				# Rise with moderate stick (40 < 60 water-jump threshold, so
				# no accidental trigger); at the surface, one decisive A
				# press with hard stick_up.
				var depth := player.global_position.y
				if depth > -0.05 and not aux_flag:
					in_sy = 1.0
					_press(BTN_A, 2)
					aux_flag = true
				else:
					in_sy = 0.5
					if seg_tick % 20 == 5:
						_press(BTN_A, 2)
