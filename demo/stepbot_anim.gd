extends Node3D
## StepBot procedural animator: drives the reference character's limbs
## from the current action ID. All motion is original, authored for StepKit.

@onready var body: MeshInstance3D = $Body
@onready var head_pivot: Node3D = $HeadPivot
@onready var arm_l: Node3D = $ArmLPivot
@onready var arm_r: Node3D = $ArmRPivot
@onready var leg_l: Node3D = $LegLPivot
@onready var leg_r: Node3D = $LegRPivot

var action := 0x0C400201
var time := 0.0
var speed := 0.0  # forward speed, for walk cycle rate

func _process(delta: float) -> void:
	time += delta
	_animate(delta)

func _animate(_delta: float) -> void:
	# Default: relaxed pose.
	var leg_swing := 0.0
	var arm_swing := 0.0
	var crouch := 0.0
	var tuck := 0.0
	var stroke := 0.0
	
	match action:
		0x0C400201:  # Idle: gentle bob.
			body.position.y = 0.85 + sin(time * 2.0) * 0.02
		0x40000440, 0x40000443, 0x40000445:  # Walking/Turning/Decel: swing.
			var rate = 6.0 + speed * 0.3
			leg_swing = sin(time * rate) * 0.6
			arm_swing = sin(time * rate + PI) * 0.5
			body.position.y = 0.85 + abs(sin(time * rate)) * 0.03
		0x03000880, 0x03000881, 0x01000882:  # Jumps: tuck.
			tuck = 0.7
			arm_l.rotation.x = -2.5
			arm_r.rotation.x = -2.5
		0x01000883:  # Backflip: tuck hard, arms out.
			tuck = 1.0
			arm_l.rotation.z = 1.5
			arm_r.rotation.z = -1.5
		0x01000887:  # Side flip: lean.
			tuck = 0.5
			rotation.z = 0.3
		0x03000888, 0x0188088A:  # Long jump/Dive: superman.
			arm_l.rotation.x = -2.8
			arm_r.rotation.x = -2.8
			leg_l.rotation.x = 0.4
			leg_r.rotation.x = 0.4
			rotation.x = 0.5
		0x018808B0:  # Wall kick: legs push.
			tuck = 0.8
		0x0800034B:  # Ledge grab: arms up.
			arm_l.rotation.x = -2.8
			arm_r.rotation.x = -2.8
		0x40000447:  # Butt slide: sit.
			crouch = 1.0
			leg_l.rotation.x = 1.2
			leg_r.rotation.x = 1.2
		0x40000010, 0x40000011:  # Crouch/Crawl.
			crouch = 1.0
			if action == 0x40000011:
				leg_swing = sin(time * 5.0) * 0.4
		0x300022E2:  # Water plunge: streamline.
			arm_l.rotation.x = -2.8
			arm_r.rotation.x = -2.8
		0x40000E3:  # Swimming: stroke.
			stroke = sin(time * 4.0)
			arm_l.rotation.x = -1.5 + stroke * 0.8
			arm_r.rotation.x = -1.5 - stroke * 0.8
			leg_swing = sin(time * 4.0) * 0.3
		_:
			body.position.y = 0.85
	
	# Apply procedural pose.
	if leg_swing != 0.0:
		leg_l.rotation.x = leg_swing
		leg_r.rotation.x = -leg_swing
	if arm_swing != 0.0 and stroke == 0.0:
		arm_l.rotation.x = arm_swing
		arm_r.rotation.x = -arm_swing
	if tuck > 0.0:
		leg_l.rotation.x = tuck * 0.8
		leg_r.rotation.x = tuck * 0.8
	if crouch > 0.0:
		body.position.y = 0.85 - 0.25 * crouch
		head_pivot.position.y = 1.35 - 0.2 * crouch
	# Reset rotation not used.
	if action != 0x01000887 and action != 0x03000888 and action != 0x0188088A:
		rotation.x = 0.0
		rotation.z = 0.0
