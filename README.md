# StepKit — SM64-style character controller for Godot 4.7+

A clean-room, data-driven character controller: a pure-Rust fixed-30Hz
simulation core (`stepkit-core`) with a Godot 4.7+ GDExtension binding
(`stepkit-godot`), an animation timing manifest + validator
(`stepkit-anim`), and a trace/scenario test harness (`stepkit-trace`,
`stepkit-cli`).

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
| `traces/scenarios` | 33 input scenarios (RON) |
| `traces/golden` | Golden traces (regenerate with `tools/regen-golden`) |
| `spec/` | Per-action behavior pages + `constants.toml` provenance |
| `anim/` | Timing manifest + reference clips |
| `demo/` | Godot 4.7 demo project |
| `docs/` | User guide, legal, clean-room process |

## Clean-room process

Every gameplay number is either:
- **Verified**: sourced from public community documentation (ukikipedia),
  with page, revision, and retrieval date in `spec/constants.toml`; or
- **(verify)**: a working assumption, clearly marked, never presented as exact.

No decompilation, ROM data, or proprietary source was read or used.
See `docs/legal.md` and `docs/clean-room.md`.

## Provenance

95 constants tracked. Run `python3 tools/check-provenance.py`.

## License

MIT — see `LICENSE`.
