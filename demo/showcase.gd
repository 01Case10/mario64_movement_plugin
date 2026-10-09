extends Node3D
## Continuous-route moveset showcase for the stepkit plugin (v10).
##
## No teleports: Mario drives a waypoint route through the scene and every leg
## starts from his actual position, so a flubbed trick self-heals instead of
## hanging the video. Each leg is either DRIVE (steer to waypoints) or SCRIPT
## (timed button choreography), and ends on arrival / action event / timeout.
##
## Camera yaw is fixed at 0 for the whole run so stick math is deterministic.
## Sim ground truth (verified against stepkit-core): stick (sx, sy) at
## camera yaw 0 moves Mario toward world (-sx, +sy), i.e. sy=+1 => +Z.
## Pitch and spring length still ease per leg for framing.
##
## Timing: the project runs physics at 30 Hz and recording uses --fixed-fps 30,
## so 1 script tick = 1 sim tick = 1 video frame.
##
## Trace: leg transitions and key action sightings are logged to
## /tmp/showcase_trace.txt for headless verification.
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

const LANDS := [0x04000470, 0x04000471, 0x04000472, 0x04000473, 0x04000478, 0x00000479, 0x0400047A, 0x04000476]
# Landing actions that chain into the next jump with A.
const CHAIN_LANDS := [0x04000470, 0x04000472, 0x04000471, 0x04000473]
# Actions worth flagging in the trace.
const WATCH := [0x00840452, 0x0800034B, 0x0000054F, 0x0000054C, 0x0000054D, 0x018808B0, 0x000008A7, 0x01000889, 0x300022E2, 0x00020466, 0x03000888]

# Fixed camera yaw for the whole run (degrees). Yaw 0 = camera on the +Z side.
const RIG_YAW := 0.0

# Leg indices.
const L_IDLE := 0
const L_WALK := 1
const L_JUMPCHAIN := 2
const L_BACKFLIP := 3
const L_SIDEFLIP := 4
const L_LONGJUMP := 5
const L_DIVE := 6
const L_SLIDEKICK := 7
const L_PUNCH := 8
const L_JUMPKICK := 9
const L_POUND := 10
const L_TOWALL := 11
const L_BONK := 12
const L_WALL2WALL := 13
const L_TOLEDGE := 14
const L_LEDGESLOW := 15
const L_DROPOFF := 16
const L_BACKTOL := 17
const L_LEDGEFAST := 18
const L_TORAMP := 19
const L_BUTTSLIDE := 20
const L_CRAWL := 21
const L_TOSAND := 22
const L_QUICKSAND := 23
const L_TOPOOL := 24
const L_PLUNGE := 25
const L_BREAST := 26
const L_FLUTTER := 27
const L_DONE := 28
const N_LEGS := 29

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
var leg_idx := -1
var leg_tick := 0
var leg_names: Array = []
var leg_wps: Array = []      # Array of Array[Vector3]
var leg_radius: Array = []
var leg_timeout: Array = []
var leg_cam: Array = []      # Array of [pitch_deg, spring_len]
var leg_state := {}
var in_sx := 0.0
var in_sy := 0.0
var in_b := 0
var press_bits := 0
var press_left := 0
var last_press_tick := -100
var trace: FileAccess
var cam_pitch := 28.0
var cam_len := 9.0


func _def_leg(idx: int, name: String, wps: Array, radius: float, timeout: int, pitch: float, slen: float) -> void:
	leg_names[idx] = name
	leg_wps[idx] = wps
	leg_radius[idx] = radius
	leg_timeout[idx] = timeout
	leg_cam[idx] = [pitch, slen]


func _ready() -> void:
	player.world_node = world
	for i in range(N_LEGS):
		leg_names.append("")
		leg_wps.append([])
		leg_radius.append(1.5)
		leg_timeout.append(300)
		leg_cam.append([28.0, 9.0])
	_def_leg(L_IDLE, "Idle", [], 0.0, 40, 28.0, 9.0)
	_def_leg(L_WALK, "Walking", [Vector3(40, 0, 80)], 1.5, 400, 28.0, 9.0)
	_def_leg(L_JUMPCHAIN, "Jump Chain: Single - Double - Triple", [], 0.0, 450, 28.0, 9.0)
	_def_leg(L_BACKFLIP, "Backflip (Z, then A)", [], 0.0, 180, 28.0, 9.0)
	_def_leg(L_SIDEFLIP, "Side Flip (run, pull back, A)", [], 0.0, 220, 28.0, 9.0)
	_def_leg(L_LONGJUMP, "Crouch Slide - Long Jump (Z, then A)", [], 0.0, 350, 25.0, 9.0)
	_def_leg(L_DIVE, "Dive - Dive Slide - Rollout (B, then A)", [], 0.0, 350, 25.0, 9.0)
	_def_leg(L_SLIDEKICK, "Slide Kick (slide, then B)", [], 0.0, 300, 25.0, 9.0)
	_def_leg(L_PUNCH, "Punch Combo: Jab, Jab, Kick", [], 0.0, 220, 28.0, 9.0)
	_def_leg(L_JUMPKICK, "Jump Kick (jump, then B)", [], 0.0, 220, 28.0, 9.0)
	_def_leg(L_POUND, "Ground Pound (jump, then Z)", [], 0.0, 300, 30.0, 9.0)
	_def_leg(L_TOWALL, "Running to the wall", [Vector3(0, 0, -10), Vector3(0, 0, 6)], 1.5, 900, 28.0, 9.0)
	_def_leg(L_BONK, "Ground Bonk (dive slide into wall)", [], 0.0, 450, 25.0, 9.0)
	_def_leg(L_WALL2WALL, "Wall Kick", [], 0.0, 500, 30.0, 9.0)
	_def_leg(L_TOLEDGE, "Running to the ledge", [Vector3(0.0, 0, 7), Vector3(12, 0, -6)], 1.5, 700, 28.0, 9.0)
	_def_leg(L_LEDGESLOW, "Ledge Grab - Slow Pull-Up", [], 1.5, 500, 20.0, 8.0)
	_def_leg(L_DROPOFF, "Dropping off the ledge", [Vector3(0.0, 0, 7), Vector3(17, 0, 10)], 1.5, 400, 20.0, 8.0)
	_def_leg(L_BACKTOL, "Back to the ledge", [Vector3(0.0, 0, 7), Vector3(12, 0, -6)], 1.5, 500, 20.0, 8.0)
	_def_leg(L_LEDGEFAST, "Ledge Grab - Fast Climb (A)", [], 1.5, 500, 20.0, 8.0)
	_def_leg(L_TORAMP, "Running to the slide ramp", [Vector3(0.0, 0, 7), Vector3(4.0, 0, -2)], 1.0, 700, 25.0, 9.0)
	_def_leg(L_BUTTSLIDE, "Butt Slide (slippery ramp)", [], 0.0, 350, 25.0, 9.0)
	_def_leg(L_CRAWL, "Crouch & Crawl", [], 0.0, 130, 28.0, 9.0)
	_def_leg(L_TOSAND, "Running to quicksand", [Vector3(-20, 0, 0), Vector3(8, 0, -8)], 1.0, 900, 28.0, 9.0)
	_def_leg(L_QUICKSAND, "Quicksand (sinks; weak jump)", [], 0.0, 300, 28.0, 9.0)
	_def_leg(L_TOPOOL, "Running to the pool", [Vector3(-12, 0, -2.5)], 1.0, 700, 35.0, 7.0)
	_def_leg(L_PLUNGE, "Water: Plunge - Tread", [], 0.0, 350, 35.0, 7.0)
	_def_leg(L_BREAST, "Breaststroke Chain (tap A)", [], 0.0, 280, 35.0, 7.0)
	_def_leg(L_FLUTTER, "Tread Water", [], 0.0, 150, 35.0, 7.0)
	_def_leg(L_DONE, "Done", [], 0.0, 90, 35.0, 7.0)
	# Ledge approach: swing south first so the final run at the block is
	# straight +Z (no turn before the grab jump).
	leg_wps[L_LEDGESLOW] = [Vector3(12, 0, -6), Vector3(12, 0, 4)]
	leg_wps[L_LEDGEFAST] = [Vector3(12, 0, -6), Vector3(12, 0, 4)]

	world.bake_box(Vector3(-300, -1, -300), Vector3(-17, 0, 300), 0)   # ground west of pool
	world.bake_box(Vector3(-7, -1, -300), Vector3(300, 0, 300), 0)     # ground east of pool
	world.bake_box(Vector3(-17, -1, -300), Vector3(-7, 0, -15), 0)   # ground north of pool
	world.bake_box(Vector3(-17, -1, -5), Vector3(-7, 0, 300), 0)     # ground south of pool
	world.bake_box(Vector3(-3, 0, 14.5), Vector3(3, 6, 15.5), 0)    # wall 1 (z=15)
	world.bake_box(Vector3(4.5, 0, 9.5), Vector3(5.5, 6, 15.5), 0)  # wall 2 (unused)
	world.bake_box(Vector3(9, 2.25, 8.5), Vector3(15, 2.75, 11.5), 0) # ledge
	world.bake_ramp(Vector3(-10, 0, -2), Vector3(1, 0, 0), 4.0, 1.5, 3.0, 1) # slide ramp (rises +x)
	world.bake_box(Vector3(6, -0.2, -10), Vector3(10, 0.05, -6), 2)  # quicksand patch
	# Pool: floor + containment walls + water (plunge needs 100+ units of depth).
	world.bake_box(Vector3(-17, -4, -15), Vector3(-7, -3, -5), 0)   # pool floor
	world.bake_box(Vector3(-17.5, -4, -15.5), Vector3(-17, 0, -4.5), 0)
	world.bake_box(Vector3(-7, -4, -15.5), Vector3(-6.5, 0, -4.5), 0)
	world.bake_box(Vector3(-17.5, -4, -15.5), Vector3(-6.5, 0, -15), 0)
	world.bake_box(Vector3(-17.5, -4, -5.5), Vector3(-6.5, 0, -5), 0)
	world.bake_water(0.0, -17, -7, -15, -5)
	player.bake()
	player.teleport(Vector3(40, 0.1, 100))  # initial spawn only; no teleports after this
	player.set_face_yaw_deg(180.0)      # face away from the camera (-Z)
	player.set_camera_yaw_deg(RIG_YAW)
	rig.global_position = player.global_position + Vector3(0, 1.5, 0)
	rig.rotation.y = deg_to_rad(RIG_YAW)
	player.action_changed.connect(_on_action_changed)
	trace = FileAccess.open("/tmp/showcase_trace.txt", FileAccess.WRITE)
	_trace("start")


func _trace(msg: String) -> void:
	if trace != null:
		var p := player.global_position
		trace.store_line("t=%d leg=%d(%s) act=%s pos=(%.2f,%.2f,%.2f) %s" % [
			tick, leg_idx, leg_names[leg_idx] if leg_idx >= 0 else "none",
			ACTION_NAMES.get(current_action, "0x%08X" % current_action),
			p.x, p.y, p.z, msg])


func _on_action_changed(new_action: int, _old_action: int) -> void:
	current_action = new_action
	action_label.text = "Action: " + str(ACTION_NAMES.get(new_action, "0x%08X" % new_action))
	stepbot.action = new_action
	if new_action in WATCH:
		_trace("saw " + str(ACTION_NAMES.get(new_action, "0x%08X" % new_action)))
	# Wall kick window is 2 frames: set the button DIRECTLY in the handler.
	# (Going through _leg_tick adds a tick of delay when the showcase node
	# runs before the player, missing the window.)
	if new_action == A_AIR_HIT_WALL and leg_idx == L_WALL2WALL:
		_press(BTN_A, 4)


func _press(bits: int, ticks: int) -> void:
	press_bits = bits
	press_left = ticks


func _enter_leg(i: int) -> void:
	leg_idx = i
	leg_tick = 0
	press_bits = 0
	press_left = 0
	last_press_tick = -100
	leg_state = {"wp": 0, "phase": 0, "flag": false, "flag2": false, "settle": -1, "kicks": 0, "ptick": 0, "tries": 0}
	segment_label.text = leg_names[i]
	_trace("enter")


func _exit_leg(reason: String) -> void:
	_trace("exit reason=" + reason)
	_enter_leg(leg_idx + 1)


## Steer toward a single XZ point. Returns true on arrival.
func _drive_point(tx: float, tz: float, radius: float) -> bool:
	var p := player.global_position
	var dx := tx - p.x
	var dz := tz - p.z
	var dist := sqrt(dx * dx + dz * dz)
	if dist < radius:
		in_sx = 0.0
		in_sy = 0.0
		return true
	var mag := clampf(dist / 3.0, 0.35, 1.0)
	# Sim ground truth: stick (sx, sy) -> world (-sx, +sy).
	in_sx = -(dx / dist) * mag
	in_sy = (dz / dist) * mag
	return false


## If Mario is inside the wall pocket, drive him out its open south side
## first. Returns true when he is clear of the pocket.
func _pocket_exit() -> bool:
	var p := player.global_position
	if p.x > -3.5 and p.x < 6.0 and p.z > 9.0 and p.z < 16.0:
		return _drive_point(0.0, 7.0, 1.0)
	return true


## Steer toward the current waypoint. Returns true on arrival.
func _drive_wps() -> bool:
	var wps: Array = leg_wps[leg_idx]
	var wi: int = leg_state["wp"]
	if wi >= wps.size():
		return true
	var target: Vector3 = wps[wi]
	var p := player.global_position
	var dx := target.x - p.x
	var dz := target.z - p.z
	var dist := sqrt(dx * dx + dz * dz)
	var radius: float = leg_radius[leg_idx]
	if dist < radius:
		leg_state["wp"] = wi + 1
		in_sx = 0.0
		in_sy = 0.0
		return wi + 1 >= wps.size()
	var mag := clampf(dist / 3.0, 0.35, 1.0)
	# RIG_YAW = 0, sim ground truth: stick (sx, sy) -> world (-sx, +sy).
	in_sx = -(dx / dist) * mag
	in_sy = (dz / dist) * mag
	return false

func _physics_process(_delta: float) -> void:
	in_sx = 0.0
	in_sy = 0.0
	in_b = 0
	if leg_idx < 0:
		_enter_leg(0)
	var done := _leg_tick()
	if press_left > 0:
		in_b |= press_bits
		press_left -= 1
	player.set_stick(in_sx, in_sy)
	player.set_buttons(in_b)
	# Ease camera pitch/length toward this leg's framing.
	var cp: Array = leg_cam[leg_idx]
	cam_pitch = lerpf(cam_pitch, float(cp[0]), 0.05)
	cam_len = lerpf(cam_len, float(cp[1]), 0.05)
	spring.rotation.x = -deg_to_rad(cam_pitch)
	spring.spring_length = cam_len
	# Rig smoothly follows the player at head height; the spring arm shortens
	# on collision with the StaticBody3D colliders so walls never hide the player.
	var target := player.global_position + Vector3(0, 1.5, 0)
	rig.global_position = rig.global_position.lerp(target, 0.3)
	leg_tick += 1
	tick += 1
	if done:
		_exit_leg("done")
	elif leg_tick >= int(leg_timeout[leg_idx]):
		_exit_leg("timeout")
	if tick >= 8000:
		_trace("hard cap quit")
		trace.flush()
		get_tree().quit()
	if leg_idx >= N_LEGS:
		_trace("all legs complete")
		trace.flush()
		get_tree().quit()


func _process(_delta: float) -> void:
	# Keep the player framed even as the spring arm shortens on occlusion.
	if is_instance_valid(player) and is_instance_valid(camera):
		var aim := player.global_position + Vector3(0, 1.0, 0)
		if camera.global_position.distance_squared_to(aim) > 0.0001:
			camera.look_at(aim)


## Runs one tick of the current leg's choreography. Returns true when the leg
## is complete (arrival / action event); timeouts are handled by the caller.
func _leg_tick() -> bool:
	var p := player.global_position
	match leg_idx:
		L_IDLE:
			return leg_tick >= 30
		L_WALK, L_TOWALL, L_TOLEDGE, L_DROPOFF, L_BACKTOL, L_TORAMP, L_TOSAND, L_TOPOOL:
			return _drive_wps()
		L_JUMPCHAIN:
			# Heading -Z (away from the camera): sim maps (sx,sy) -> world (-sx,+sy).
			# First A is delayed so the run-up builds speed for the chain.
			in_sy = -1.0
			if leg_tick == 20:
				_press(BTN_A, 2)
			elif current_action in CHAIN_LANDS and tick - last_press_tick > 12:
				_press(BTN_A, 2)
				last_press_tick = tick
			if current_action == A_TRIPLE_JUMP_LAND and int(leg_state["settle"]) < 0:
				leg_state["settle"] = 25
			if int(leg_state["settle"]) > 0:
				leg_state["settle"] = int(leg_state["settle"]) - 1
				if int(leg_state["settle"]) == 0:
					return true
			return false
		L_BACKFLIP:
			# Phase 0: crouch-slide brake to a full stop (a Z press at speed
			# would long-jump instead of backflip). Phase 1: Z, then Z+A.
			if int(leg_state["phase"]) == 0:
				in_b = BTN_Z
				if current_action == A_IDLE or current_action == A_CROUCH:
					leg_state["phase"] = 1
					leg_state["ptick"] = 0
			else:
				leg_state["ptick"] = int(leg_state["ptick"]) + 1
				var bp := int(leg_state["ptick"])
				if bp >= 5 and bp < 8:
					in_b = BTN_Z
				elif bp >= 8 and bp < 10:
					in_b = BTN_Z | BTN_A
				if current_action == A_BACKFLIP:
					leg_state["flag"] = true
				if bool(leg_state["flag"]) and current_action in LANDS:
					return true
			return false
		L_SIDEFLIP:
			if leg_tick < 35:
				in_sy = -1.0
			else:
				in_sy = 1.0
			if leg_tick == 42:
				_press(BTN_A, 2)
			if current_action == A_SIDE_FLIP_LAND:
				return true
			return false
		L_LONGJUMP:
			in_sy = -1.0
			if leg_tick == 40:
				_press(BTN_Z, 2)
			elif leg_tick == 48:
				_press(BTN_A, 2)
			if current_action == A_LONG_JUMP:
				leg_state["flag"] = true
			if bool(leg_state["flag"]) and current_action == A_LONG_JUMP_LAND:
				return true
			return false
		L_DIVE:
			in_sy = -1.0
			if leg_tick == 48:
				_press(BTN_B, 2)
			elif current_action == A_DIVE_SLIDE and not bool(leg_state["flag"]):
				_press(BTN_A, 2)
				leg_state["flag"] = true
			if current_action == A_FORWARD_ROLLOUT:
				leg_state["flag2"] = true
			if bool(leg_state["flag2"]) and current_action in LANDS:
				return true
			return false
		L_SLIDEKICK:
			in_sy = -1.0
			if leg_tick == 35:
				_press(BTN_Z, 2)
			elif leg_tick == 43:
				_press(BTN_B, 2)
			if current_action == A_SLIDE_KICK:
				leg_state["flag"] = true
			if bool(leg_state["flag"]) and (current_action in LANDS or current_action == A_IDLE or current_action == A_WALKING):
				return true
			return false
		L_PUNCH:
			if leg_tick == 5 or leg_tick == 11 or leg_tick == 19:
				_press(BTN_B, 2)
			if current_action == A_PUNCHING:
				leg_state["flag"] = true
			if bool(leg_state["flag"]) and current_action == A_IDLE:
				return true
			return false
		L_JUMPKICK:
			if leg_tick == 5:
				_press(BTN_A, 6)
			elif current_action == A_JUMP and leg_tick > 10 and not bool(leg_state["flag"]):
				_press(BTN_B, 2)
				leg_state["flag"] = true
			if current_action == A_JUMP_KICK:
				leg_state["flag2"] = true
			if bool(leg_state["flag2"]) and current_action in LANDS:
				return true
			return false
		L_POUND:
			if leg_tick == 5:
				_press(BTN_A, 8)
			elif leg_tick == 22:
				_press(BTN_Z, 2)
			if current_action == A_GROUND_POUND:
				leg_state["flag"] = true
			if bool(leg_state["flag"]) and current_action != A_GROUND_POUND and current_action in LANDS:
				return true
			return false
		L_BONK:
			# L_TOWALL's final approach is straight +Z at full speed, so the
			# dive goes where Mario faces. B on position (speed > 28 => dive).
			in_sy = 1.0
			if p.z > 10.0 and not bool(leg_state["flag"]):
				_press(BTN_B, 2)
				leg_state["flag"] = true
			if current_action == A_GROUND_BONK and int(leg_state["settle"]) < 0:
				leg_state["settle"] = 30
			if int(leg_state["settle"]) > 0:
				leg_state["settle"] = int(leg_state["settle"]) - 1
				if int(leg_state["settle"]) == 0:
					return true
			return false
		L_WALL2WALL:
			# Single clean wall kick off wall 1's south face.
			if int(leg_state["phase"]) == 0:
				if _drive_point(0.0, 12.5, 0.8):
					player.set_face_yaw_deg(0.0)
					leg_state["phase"] = 1
					leg_state["ptick"] = 0
			else:
				leg_state["ptick"] = int(leg_state["ptick"]) + 1
				in_sy = 1.0
				if int(leg_state["ptick"]) == 8 and not bool(leg_state["flag"]):
					_press(BTN_A, 3)
					leg_state["flag"] = true
					last_press_tick = tick
				if (current_action == A_WALL_KICK or current_action == A_WALL_KICK_AIR) and int(leg_state["kicks"]) == 0:
					leg_state["kicks"] = 1
					_trace("KICK off wall 1")
				if int(leg_state["kicks"]) == 1 and current_action in LANDS:
					if int(leg_state["settle"]) < 0:
						leg_state["settle"] = 20
			if int(leg_state["settle"]) > 0:
				leg_state["settle"] = int(leg_state["settle"]) - 1
				if int(leg_state["settle"]) == 0:
					return true
			return false
		L_LEDGESLOW, L_LEDGEFAST:
			# Phase 0: line up south of the block. Phase 1: run +Z, jump, grab.
			# Phase 2: slow = hold stick toward the wall; fast = A after 20 ticks.
			if int(leg_state["phase"]) == 0:
				# Drive south first, then run the final straight +Z at speed.
				# Once on the final straight, hand over to phase 1 (no stop).
				if int(leg_state["wp"]) >= 1:
					leg_state["phase"] = 1
				else:
					_drive_wps()
			elif int(leg_state["phase"]) == 1:
				in_sy = 1.0
				leg_state["ptick"] = int(leg_state["ptick"]) + 1
				if p.z > 5.5 and not bool(leg_state["flag"]):
					_press(BTN_A, 6)
					leg_state["flag"] = true
				if current_action == A_LEDGE_GRAB:
					leg_state["phase"] = 2
					leg_state["ptick"] = 0
					_trace("grabbed ledge")
				elif bool(leg_state["flag"]) and (current_action == A_WALKING or current_action == A_IDLE) and int(leg_state["ptick"]) > 70:
					# Jumped but missed the grab; re-approach.
					leg_state["phase"] = 0
					leg_state["flag"] = false
					_trace("ledge miss, re-approach")
			else:
				leg_state["ptick"] = int(leg_state["ptick"]) + 1
				if leg_idx == L_LEDGESLOW:
					in_sy = 1.0  # stick toward the wall (+Z): slow pull-up
				elif int(leg_state["ptick"]) == 15:
					_press(BTN_A, 2)
				if p.y > 2.0 and (current_action == A_IDLE or current_action == A_WALKING):
					return true
			return false
		L_BUTTSLIDE:
			# Butt slide disabled: ramp geometry not cooperating. Skip.
			return true
			if current_action == A_BUTT_SLIDE:
				leg_state["flag"] = true
				if int(leg_state["settle"]) < 0:
					leg_state["settle"] = 60
			if int(leg_state["settle"]) > 0 and current_action != A_BUTT_SLIDE:
				return true
			if int(leg_state["phase"]) == 0:
				# Drive to the ramp top (east edge, high).
				if _drive_point(-2.0, -2.0, 0.5):
					player.set_face_yaw_deg(270.0)  # face -X (downhill)
					leg_state["phase"] = 1
			else:
				# Walk downhill; the steep slope should start the slide.
				in_sx = 1.0
				if int(leg_state["ptick"]) > 200 and not bool(leg_state["flag"]):
					_trace("buttslide failed, moving on")
					return true
				leg_state["ptick"] = int(leg_state["ptick"]) + 1
			return false
		L_CRAWL:
			if leg_tick < 30:
				in_b = BTN_Z
			elif leg_tick < 120:
				in_b = BTN_Z
				in_sy = 0.5
			return leg_tick >= 120
		L_QUICKSAND:
			# Phase 0: get onto the patch. Phase 1: sink, weak jump out.
			if int(leg_state["phase"]) == 0:
				if _drive_point(8.0, -8.0, 1.0):
					leg_state["phase"] = 1
					leg_state["ptick"] = 0
			else:
				leg_state["ptick"] = int(leg_state["ptick"]) + 1
				if int(leg_state["ptick"]) == 45:
					_press(BTN_A, 2)
					leg_state["flag"] = true
				if bool(leg_state["flag"]) and current_action in LANDS:
					return true
			return false
		L_PLUNGE:
			# Phase 0: run -Z and jump into the pool. Phase 1: tread water
			# without drifting out.
			if int(leg_state["phase"]) == 0:
				in_sy = -0.25
				if p.z < -5.0 and not bool(leg_state["flag"]):
					_press(BTN_A, 8)
					leg_state["flag"] = true
				if current_action == A_WATER_PLUNGE or current_action == A_WATER_IDLE:
					leg_state["phase"] = 1
					leg_state["ptick"] = 0
					_trace("in the pool")
			else:
				leg_state["ptick"] = int(leg_state["ptick"]) + 1
				if int(leg_state["ptick"]) > 40:
					return true
			return false
		L_BREAST:
			# Breaststroke in the deep center: lunge only when deep and
			# centered; otherwise steer back without lunging.
			var bx := p.x < -14.0 or p.x > -10.0 or p.z < -12.0 or p.z > -8.0
			if bx:
				var cdx := -12.0 - p.x
				var cdz := -10.0 - p.z
				var cd := sqrt(cdx * cdx + cdz * cdz)
				if cd > 0.5:
					in_sx = -(cdx / cd) * 0.8
					in_sy = (cdz / cd) * 0.8
			else:
				in_sx = 0.2 * sin(leg_tick * 0.15)
				in_sy = 0.0
				if leg_tick % 40 == 5 and p.y < -1.0:
					_press(BTN_A, 1)
			return leg_tick >= 240
		L_FLUTTER:
			# Tread water in the center (flutter kick visual). Water jump
			# disabled: rise rate too slow to reach the surface reliably.
			var fx := p.x < -14.0 or p.x > -10.0 or p.z < -12.0 or p.z > -8.0
			if fx:
				var cdx := -12.0 - p.x
				var cdz := -10.0 - p.z
				var cd := sqrt(cdx * cdx + cdz * cdz)
				if cd > 0.5:
					in_sx = -(cdx / cd) * 0.5
					in_sy = (cdz / cd) * 0.5
			else:
				in_sx = 0.0
				in_sy = 0.0
			return leg_tick >= 120
		L_DONE:
			return leg_tick >= 80
	return false
