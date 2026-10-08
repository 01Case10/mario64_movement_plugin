# StepKit Demo (Godot 4.7+)

A playable demo of the stepkit character controller.

## Run

1. Open this folder in Godot 4.7+.
2. Press F5 (or run `main.tscn`).

The prebuilt `bin/` libraries are for Linux x86_64. Rebuild for your
platform with:

```
cargo build --release -p stepkit-godot
cp target/release/libstepkit_godot.so demo/bin/libstepkit_godot.linux.release.x86_64.so
```

## Controls

- **WASD / arrows**: move (stick; W is forward)
- **Space**: A (jump; press again in air for double/triple jump, wall kick near a wall)
- **Shift**: B (dive)
- **C**: Z (ground pound in air, crouch on ground)

## Level

- Flat floor, a 4m wall (jump at it, then Space again to wall-kick),
  and a slide ramp (walk onto it to butt-slide).

## Verify headless

```
godot --headless --path . res://verify.tscn
```

Expected: `VERIFY ALL PASS`.

## Notes

- The character simulates at a fixed 30 Hz; rendering interpolates.
- 1 Godot meter = 100 SM64 units (`unit_scale = 0.01`).
- The capsule is a placeholder; the animated mannequin lands in phase 6.
