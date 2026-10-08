# StepKit user guide

## The character node

Add a `StepChar3D` to your scene. Set its `world_node` to a `StepWorld3D`
(in the editor inspector or from code). The character simulates at a fixed
30 Hz and interpolates its transform for smooth rendering.

```gdscript
@onready var player: StepChar3D = $StepChar3D

func _physics_process(_delta):
    var sx = Input.get_axis("move_left", "move_right")
    var sy = Input.get_axis("move_back", "move_forward")
    player.set_stick(sx, sy)
    var b = 0
    if Input.is_action_pressed("jump"): b |= StepChar3D.BTN_A
    if Input.is_action_pressed("dive"): b |= StepChar3D.BTN_B
    if Input.is_action_pressed("pound"): b |= StepChar3D.BTN_Z
    player.set_buttons(b)
```

Button presses are latched: a tap visible for one rendered frame still
reaches the 30 Hz core.

## Controller input

The plugin is input-agnostic — it takes a stick vector and a button bitmask.
For gamepad support without configuring Project Settings, use the bundled
`StepKitInput` helper (`demo/stepkit_input.gd`):

```gdscript
const StepKitInput = preload("res://stepkit_input.gd")

func _physics_process(delta):
    # Polls the gamepad directly; falls back to the InputMap
    # (WASD/Space/Shift/C) when no gamepad is connected.
    # Right stick rotates the camera yaw.
    StepKitInput.drive(player, player.get_camera_yaw_deg(), delta)
```

Controller layout (Godot Xbox labels):

| Gamepad              | Action | Button bit |
|----------------------|--------|------------|
| Left stick           | Move   | (stick)    |
| South (A / Cross)    | Jump   | `BTN_A` (1) |
| West (X / Square)    | Dive   | `BTN_B` (2) |
| East (B / Circle)    | Pound  | `BTN_Z` (4) |
| Right stick X        | Camera yaw | —      |

The demo's `project.godot` also defines these as InputMap actions
(`move_left/right/back/forward`, `jump`, `dive`, `pound`) with both keyboard
and gamepad bindings, so `Input.get_axis` / `Input.is_action_pressed` work
with a controller out of the box.

## The world

`StepWorld3D` bakes collision. Call from GDScript (usually `_ready`):

```gdscript
world.bake_box(min: Vector3, max: Vector3, kind: int)
world.bake_ramp(origin, dir, length, height, width, kind)
world.bake_mesh(node: Node3D, mesh: ArrayMesh, kind: int)
```

Kinds: `0` = default, `1` = slide (butt-slide), `2` = quicksand.
Then `player.bake()` to copy the world into the character.

Units: 1 Godot meter = 100 simulation units (`unit_scale = 0.01` on both
nodes; they must match).

## Signals

- `action_changed(new_action: int, old_action: int)` — IDs in `spec/constants.toml`
- `jumped(velocity_y: float)`, `landed(fall_speed: float)`
- `wall_hit()`, `ledge_grabbed()`

## Tuning

`StepParams` (a Resource) exposes the key tunables; defaults are the
documented values. Changing a param marks the run non-faithful for the
harness.

## Animation

`StepChar3D.get_anim_slot()` returns the current timeline slot; drive your
`AnimationPlayer` from it. Validate clips with:

```bash
stepkit anim validate --manifest anim/timing_manifest.toml --clips my_clips/
```
