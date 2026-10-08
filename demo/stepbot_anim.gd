extends Node3D
## StepBot procedural animator: drives the reference character's limbs
## from the current action ID. All motion is original, authored for StepKit.
## Action IDs match crates/stepkit-core/src/state.rs `impl ActionId`
## (water states: crates/stepkit-core/src/actions/water.rs `id`).

@onready var body: MeshInstance3D = $Body
@onready var head_pivot: Node3D = $HeadPivot
@onready var arm_l: Node3D = $ArmLPivot
@onready var arm_r: Node3D = $ArmRPivot
@onready var elbow_l: Node3D = $ArmLPivot/ElbowLPivot
@onready var elbow_r: Node3D = $ArmRPivot/ElbowRPivot
@onready var leg_l: Node3D = $LegLPivot
@onready var leg_r: Node3D = $LegRPivot

var action := 0x0C400201
var time := 0.0
var speed := 0.0  # forward speed, for walk cycle rate

func _process(delta: float) -> void:
	time += delta
	_animate(delta)

func _animate(_delta: float) -> void:
	# Reset to neutral every frame; each action then sets its own pose.
	rotation.x = 0.0
	rotation.z = 0.0
	arm_l.rotation = Vector3.ZERO
	arm_r.rotation = Vector3.ZERO
	elbow_l.rotation = Vector3.ZERO
	elbow_r.rotation = Vector3.ZERO
	leg_l.rotation = Vector3.ZERO
	leg_r.rotation = Vector3.ZERO
	body.position.y = 0.85
	head_pivot.position.y = 1.35

	var leg_swing := 0.0
	var arm_swing := 0.0

	match action:
		0x0C400201:  # Idle: gentle bob.
			body.position.y = 0.85 + sin(time * 2.0) * 0.02
		0x04000440, 0x00000443, 0x00000444, 0x0400044A:  # Walk / turning / finish / decel.
			var rate = 6.0 + speed * 0.3
			leg_swing = sin(time * rate) * 0.6
			arm_swing = sin(time * rate + PI) * 0.5
			body.position.y = 0.85 + abs(sin(time * rate)) * 0.03
		0x04000445:  # Braking: lean back, arms out.
			rotation.x = -0.3
			arm_l.rotation.z = 0.8
			arm_r.rotation.z = -0.8
			body.position.y = 0.8
		0x04000470, 0x04000471, 0x04000472, 0x04000473, \
		0x04000478, 0x00000479, 0x0400047A, 0x04000476:
			# All landings (jump/freefall/double/sideflip/triple/long/backflip/quicksand):
			# slight crouch.
			body.position.y = 0.72
			head_pivot.position.y = 1.25
			leg_l.rotation.x = 0.5
			leg_r.rotation.x = 0.5
		0x03000880, 0x03000881, 0x01000882, 0x03000885:  # Jump / double / triple / steep: tuck (negative = knees forward).
			leg_l.rotation.x = -0.6
			leg_r.rotation.x = -0.6
			arm_l.rotation.x = -2.5
			arm_r.rotation.x = -2.5
		0x01000883:  # Backflip: tuck hard, arms out.
			leg_l.rotation.x = -0.9
			leg_r.rotation.x = -0.9
			arm_l.rotation.z = 1.5
			arm_r.rotation.z = -1.5
		0x01000887:  # Side flip: lean.
			leg_l.rotation.x = -0.4
			leg_r.rotation.x = -0.4
			rotation.z = 0.3
		0x03000888, 0x0188088A:  # Long jump / dive (air): superman.
			# Pitch 0.32 (not 0.5) and arms -2.9 (not -2.8): the full 0.5 pitch
			# swings the head/hands 0.7-1.0 in front of the root, but the sim
			# only guarantees 0.5 wall clearance — the visual then clips walls
			# on dive impacts. Tighter pose keeps the visual inside.
			arm_l.rotation.x = -2.9
			arm_r.rotation.x = -2.9
			leg_l.rotation.x = 0.4
			leg_r.rotation.x = 0.4
			rotation.x = 0.32
		0x00880456:  # Dive slide (ground): low slide, arms trailing.
			# Arms trail BACK (+0.5), not forward: extended superman arms put
			# the hands 0.7+ in front of the root, clipping walls the sim
			# holds 0.5 clear of. Trailing arms stay inside.
			arm_l.rotation.x = 0.5
			arm_r.rotation.x = 0.5
			leg_l.rotation.x = 0.4
			leg_r.rotation.x = 0.4
			rotation.x = 0.22
		0x018008AA, 0x0080045A:  # Slide kick (air) / slide-kick slide: leg out.
			leg_l.rotation.x = -1.2
			arm_l.rotation.x = -2.0
			arm_r.rotation.x = -2.0
			rotation.x = 0.3
		0x018808B0, 0x03000886:  # Wall kick / wall-kick air: legs push.
			leg_l.rotation.x = 0.7
			leg_r.rotation.x = 0.7
		0x400008A8:  # Ground pound: tuck hard, arms swept down.
			leg_l.rotation.x = -0.9
			leg_r.rotation.x = -0.9
			arm_l.rotation.x = 0.8
			arm_r.rotation.x = 0.8
		0x010008A6, 0x010008AD:  # Forward / backward rollout: tuck.
			leg_l.rotation.x = -0.9
			leg_r.rotation.x = -0.9
			arm_l.rotation.x = -1.0
			arm_r.rotation.x = -1.0
		0x010208B6, 0x010208B0, 0x010208B1, 0x010208B2, 0x010208B3, \
		0x00020462, 0x00020463, 0x00020460, 0x00020461, \
		0x00020464, 0x00020465, 0x00020466, 0x000008A7:
			# Knockbacks (air + ground), ground bonk, soft bonk, air hit wall: sprawl.
			# Lean is -0.15 (not -0.25): the full -0.25 swings the head 0.38
			# behind the root, which clips the wall when facing away after a
			# bonk (sim holds 0.5 m clearance).
			arm_l.rotation.z = 1.2
			arm_r.rotation.z = -1.2
			rotation.x = -0.15
		0x0800034B, 0x0000054F, 0x0000054C, 0x0000054D:
			# Ledge grab + climbs: 2-bone arm IK from sim hints (hands to the
			# edge). Falls back to FK poses if no hint is active.
			if not _solve_arm_ik():
				if action == 0x0800034B:
					# Ledge grab FK: arms up-forward to the edge. The hang is
					# 30 units outside the wall face, so -2.62 puts the hands
					# exactly at the edge (0.30 m forward reach).
					arm_l.rotation.x = -2.62
					arm_r.rotation.x = -2.62
				else:
					# Climb FK: arms straight up. Forward-reaching arms would
					# stab into the top of the ledge block.
					arm_l.rotation.x = -3.1
					arm_r.rotation.x = -3.1
		0x00840452:  # Butt slide: sit, legs forward (negative = forward/downhill).
			body.position.y = 0.6
			head_pivot.position.y = 1.15
			leg_l.rotation.x = -1.2
			leg_r.rotation.x = -1.2
		0x008C0453:  # Stomach slide: prone.
			arm_l.rotation.x = -2.8
			arm_r.rotation.x = -2.8
			leg_l.rotation.x = 0.3
			leg_r.rotation.x = 0.3
			body.position.y = 0.6
			rotation.x = 0.6
		0x04808459:  # Crouch slide: low slide, legs forward (negative = forward).
			body.position.y = 0.6
			head_pivot.position.y = 1.15
			leg_l.rotation.x = -1.0
			leg_r.rotation.x = -1.0
			rotation.x = 0.2
		0x00800380, 0x00800457:  # Punching / move punching: arm jab.
			arm_r.rotation.x = -1.6
			arm_l.rotation.x = -0.4
		0x018008AC:  # Jump kick: leg out.
			leg_r.rotation.x = -1.4
			arm_l.rotation.x = -2.0
			arm_r.rotation.x = -2.0
		0x40000010, 0x40000011, 0x0002020D:  # Crouch / crawl / in quicksand.
			body.position.y = 0.6
			head_pivot.position.y = 1.15
			if action == 0x40000011:
				leg_swing = sin(time * 5.0) * 0.4
		0x300022E2, 0x01000889:  # Water plunge / water jump: streamline.
			arm_l.rotation.x = -2.8
			arm_r.rotation.x = -2.8
		0x380022C0:  # Water idle: tread.
			var stroke = sin(time * 2.0) * 0.4
			arm_l.rotation.x = -1.2 + stroke * 0.5
			arm_r.rotation.x = -1.2 - stroke * 0.5
		0x300024D0, 0x300024D2:  # Breaststroke / flutter kick: stroke.
			var stroke2 = sin(time * 4.0)
			arm_l.rotation.x = -1.5 + stroke2 * 0.8
			arm_r.rotation.x = -1.5 - stroke2 * 0.8
			leg_swing = sin(time * 4.0) * 0.3
		0x300024D1, 0x300022C2:  # Swimming end / water action end: streamlined drift.
			arm_l.rotation.x = -2.2
			arm_r.rotation.x = -2.2

	# Apply cyclic limb motion.
	if leg_swing != 0.0:
		leg_l.rotation.x = leg_swing
		leg_r.rotation.x = -leg_swing
	if arm_swing != 0.0:
		arm_l.rotation.x = arm_swing
		arm_r.rotation.x = -arm_swing

## 2-bone arm IK from sim IK hints.
##
## Returns true if IK was solved (hints active), false to use FK fallback.
## Reads get_ik_hand_l/r from the parent StepChar3D node (Vector3.ZERO = no
## hint). Elbows bend backward/outward (natural hang); beyond max reach the
## arms fully extend toward the target (graceful release during the climb).
func _solve_arm_ik() -> bool:
	var stepkit := get_parent()
	if stepkit == null or not stepkit.has_method("get_ik_hand_l"):
		return false
	var target_l: Vector3 = stepkit.get_ik_hand_l()
	var target_r: Vector3 = stepkit.get_ik_hand_r()
	if target_l.length() < 0.001 or target_r.length() < 0.001:
		return false
	# Pole: elbows point backward (away from the wall) and slightly down.
	var facing: Vector3 = (global_transform.basis * Vector3(0, 0, 1)).normalized()
	var pole: Vector3 = (-facing + Vector3.DOWN * 0.35).normalized()
	var parent_basis: Basis = global_transform.basis
	var res_l := StepKitIK.solve_arm(
		arm_l.global_transform.origin, target_l, 0.3, 0.3, pole, parent_basis)
	var res_r := StepKitIK.solve_arm(
		arm_r.global_transform.origin, target_r, 0.3, 0.3, pole, parent_basis)
	arm_l.basis = res_l[0]
	elbow_l.basis = res_l[1]
	arm_r.basis = res_r[0]
	elbow_r.basis = res_r[1]
	return true
