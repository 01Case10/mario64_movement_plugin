# Legal-review readiness

## IP posture (current)

- Movement code in `crates/stepkit-core` (and the `oracle/` harness) is
  **derived from the SM64 decompilation** (`n64decomp/sm64`), itself
  derived from Nintendo's copyrighted game. Policy adopted by the project
  owner 2026-10-11; see `NOTICE`.
- The repository's `LICENSE` (MIT, (c) 2026 01Case10) has not been
  changed, and the MIT claim is **questionable for the derived
  portions**. The owner must resolve this; this file is not legal advice.
- No ROMs, models, or extracted assets are in the repo. The demo uses
  only primitive Godot meshes (capsule, boxes). `demo/bin/*.so` are our
  compiled Rust code.
- Neutral naming is still the rule everywhere user-visible: no Nintendo
  names or marks in crate names, node classes, docs, or product name.

## What a reviewer should check

1. `oracle/DECOMP_COMMIT`: the pinned decomp commit the code derives from.
2. `NOTICE`: derivation statement present.
3. The owner's licensing decision for the derived portions (open).
4. `git log`: port-by-port construction with `oracle diff --all` status
   recorded in `docs/parity-log.md`.
5. No binaries, ROMs, or extracted assets in the repo.

## History

Before 2026-10-11 this project used a clean-room process (no
decompilation read). That policy was retired; see `docs/clean-room.md`.
