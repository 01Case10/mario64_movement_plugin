extends RefCounted
## Drop-in gamepad + keyboard input for StepChar3D.
##
## The StepKit GDExtension is input-agnostic: it takes a stick vector and a
## button bitmask via set_stick()/set_buttons(). This helper polls a gamepad
## directly (no Project Settings InputMap required) and falls back to the
## demo's InputMap actions (WASD/Space/Shift/C) when no gamepad is present.
##
## Usage (in _physics_process):
##   const StepKitInput = preload("res://stepkit_input.gd")
##   var stick = StepKitInput.poll_stick()
##   player.set_stick(stick.x, stick.y)
##   player.set_buttons(StepKitInput.poll_buttons())
##   # Or the one-call version:
##   StepKitInput.drive(player, player.get_camera_yaw_deg(), delta)
##
## Button layout (Godot Xbox labels; device -1 = any):
##   A (jump)  = South button (A on Xbox, Cross on PlayStation)
##   B (dive)  = West button  (X on Xbox, Square on PlayStation)
##   Z (pound) = East button  (B on Xbox, Circle on PlayStation)
## Left stick = move. Right stick X = camera yaw.

const BTN_A := 1
const BTN_B := 2
const BTN_Z := 4

const DEADZONE := 0.2
const CAMERA_SPEED_DEG_PER_SEC := 180.0

# Godot joypad button indices (Xbox naming).
const JOY_SOUTH := 0  # A / Cross  -> jump
const JOY_EAST := 1   # B / Circle -> pound (Z)
const JOY_WEST := 2   # X / Square -> dive (B)

# Godot joypad axis indices.
const AXIS_LX := 0
const AXIS_LY := 1
const AXIS_RX := 2


static func _has_gamepad(device: int) -> bool:
	return Input.get_connected_joypads().has(device) or (
		device == 0 and not Input.get_connected_joypads().is_empty()
	)


static func _joy(device: int) -> int:
	# Resolve "device 0" to the first connected gamepad when 0 isn't connected.
	if device == 0 and not Input.get_connected_joypads().is_empty():
		return Input.get_connected_joypads()[0]
	return device


## Stick vector in StepChar3D space: x = right, y = forward (camera-relative).
static func poll_stick(device: int = 0) -> Vector2:
	if _has_gamepad(device):
		var d := _joy(device)
		var x := Input.get_joy_axis(d, AXIS_LX)
		var y := -Input.get_joy_axis(d, AXIS_LY)  # stick up = forward
		if Vector2(x, y).length() < DEADZONE:
			return Vector2.ZERO
		return Vector2(x, y).limit_length(1.0)
	# Fallback: the demo's InputMap (keyboard).
	var sx := Input.get_axis("move_left", "move_right")
	var sy := Input.get_axis("move_back", "move_forward")
	return Vector2(sx, sy)


## Button bitmask: BTN_A | BTN_B | BTN_Z.
static func poll_buttons(device: int = 0) -> int:
	var b := 0
	if _has_gamepad(device):
		var d := _joy(device)
		if Input.is_joy_button_pressed(d, JOY_SOUTH):
			b |= BTN_A
		if Input.is_joy_button_pressed(d, JOY_WEST):
			b |= BTN_B
		if Input.is_joy_button_pressed(d, JOY_EAST):
			b |= BTN_Z
		return b
	# Fallback: the demo's InputMap (keyboard).
	if Input.is_action_pressed("jump"):
		b |= BTN_A
	if Input.is_action_pressed("dive"):
		b |= BTN_B
	if Input.is_action_pressed("pound"):
		b |= BTN_Z
	return b


## Right-stick camera yaw. Pass the player's current camera yaw and the
## physics delta; returns the updated yaw in degrees.
static func poll_camera_yaw(current_yaw_deg: float, delta: float, device: int = 0) -> float:
	if not _has_gamepad(device):
		return current_yaw_deg
	var d := _joy(device)
	var rx := Input.get_joy_axis(d, AXIS_RX)
	if absf(rx) < DEADZONE:
		return current_yaw_deg
	return current_yaw_deg + rx * CAMERA_SPEED_DEG_PER_SEC * delta


## One-call drive: stick + buttons + camera yaw. Returns the new camera yaw.
static func drive(player: StepChar3D, camera_yaw_deg: float, delta: float, device: int = 0) -> float:
	var stick := poll_stick(device)
	player.set_stick(stick.x, stick.y)
	player.set_buttons(poll_buttons(device))
	var new_yaw := poll_camera_yaw(camera_yaw_deg, delta, device)
	player.set_camera_yaw_deg(new_yaw)
	return new_yaw
