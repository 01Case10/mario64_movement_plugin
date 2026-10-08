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
