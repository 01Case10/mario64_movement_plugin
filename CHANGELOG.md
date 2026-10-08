# Changelog

## Unreleased (phases 0-8)

### Phase 0 — Scaffold
- Cargo workspace (core, anim, trace, godot, cli), 30Hz fixed core,
  deterministic trig, CI, provenance lint.

### Phase 1 — Data pipeline
- Trace/scenario schemas, diff engine, `.m64` importer, 20 scenarios.

### Phase 2 — Core ground movement
- Wiki-sourced walking (32 target, 48 cap, 1.1 accel), SurfaceWorld,
  quarter-step collision, jumps.

### Phase 3 — Jump chain and air
- Double/triple/backflip/sideflip/long jump/dive/ground pound/crouch,
  all wiki-sourced entries.

### Phase 4 — Walls, ledges, slides
- Wall kick, ledge grab (documented geometry), butt slide, air knockback.
- Fixed air landing to respect the 78u snap window.

### Phase 5 — Godot 4.7+ binding
- `StepChar3D`/`StepWorld3D`/`StepParams`, 30Hz tick + interpolation,
  button latching, signals, playable demo (headless-verified).

### Phase 6 — Animation
- Timing manifest (21 slots), validator T01–T05/T08–T10, CLI, editor dock,
  21 reference clips.

### Phase 7 — Water
- Water plunge (verified entry), swimming (provisional).

### Phase 8 — Release
- Docs, packaging, similarity audit, legal-review readiness, release gate.
