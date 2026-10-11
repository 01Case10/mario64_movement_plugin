# StepKit — SM64-style character controller for Godot 4.7+

A decomp-derived character controller: a pure-Rust fixed-30Hz
simulation core (`stepkit-core`) with a Godot 4.7+ GDExtension binding
(`stepkit-godot`), an animation timing manifest + validator
(`stepkit-anim`), and a trace/scenario test harness (`stepkit-trace`,
`stepkit-cli`).

Movement behavior reproduces the SM64 decompilation
(`n64decomp/sm64`) frame-for-frame; a native oracle built from the
decomp's own movement code is the ground truth (see `oracle/` and
`docs/parity-log.md`).

## Quick start

**Godot (playable demo):**
1. Open `demo/` in Godot 4.7+.
2. Press F5. WASD/arrows move, Space jumps, Shift dives, C ground-pounds.

**Rust (headless):**
```bash
cargo test --workspace --exclude stepkit-godot
./target/debug/stepkit trace run --scenario traces/scenarios/swim.ron --out /tmp/out.csv
```

## Layout

| Path | What |
| --- | --- |
| `crates/stepkit-core` | Deterministic 30Hz simulation: actions, physics, collision |
| `crates/stepkit-godot` | GDExtension: `StepChar3D`, `StepWorld3D`, `StepParams` |
| `crates/stepkit-anim` | Animation timing manifest + validator (T01–T10) |
| `crates/stepkit-trace` | Trace/scenario schemas, diff engine, anchors |
| `crates/stepkit-cli` | `stepkit` CLI: trace, anim, scenario tooling |
| `traces/scenarios` | 52 input scenarios (RON) |
| `traces/golden` | Oracle golden traces in §7.3 schema (regenerate with `tools/regen-golden`) |
| `spec/` | Per-action behavior pages + `constants.toml` provenance |
| `anim/` | Timing manifest + reference clips |
| `demo/` | Godot 4.7 demo project |
| `docs/` | User guide, legal, clean-room process |

## Decomp-derived process

Gameplay behavior is reproduced from the SM64 decompilation
(`n64decomp/sm64`, commit pinned in `oracle/DECOMP_COMMIT`). Movement
logic in `stepkit-core` is ported from the decomp's movement sources;
the native oracle in `oracle/` compiles that same source and is the
ground truth for differential tests (`stepkit oracle diff --all`).

Previously this project used a clean-room process (no decompilation
read). That policy was retired by the project owner on 2026-10-11 in
favor of decomp parity (see `docs/clean-room.md` for the history and
`docs/parity-log.md` for per-scenario status).

## Provenance

`spec/constants.toml` (deprecated) previously tracked the provenance of
each gameplay constant. It is kept until its values are superseded by
the ported code, then removed. The authority for behavior is now the
oracle diff, not the constants table. Run `stepkit oracle diff --all`.

## License

MIT — see `LICENSE`.

**Note:** the movement code in `stepkit-core` is derived from the SM64
decompilation (`n64decomp/sm64`), which is itself derived from Nintendo's
copyrighted game. The MIT license claim on this repository is
questionable for the derived portions; the project owner has been
flagged to resolve this (see `NOTICE` and `docs/legal.md`). This is not
legal advice.
