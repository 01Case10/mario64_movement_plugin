# Legal-review readiness

## IP posture

- All code is original, written for this project (MIT, (c) 2026 01Case10).
- No Nintendo code, assets, models, animations, or ROM data are included
  or referenced.
- Action IDs that match community-documented values are facts about
  observable behavior (not copyrightable expression); IDs we could not
  verify use a provisional bit-30 scheme.
- The demo uses only primitive Godot meshes (capsule, boxes).

## What a reviewer should check

1. `spec/constants.toml`: every `wiki` source has page/revision/retrieved.
2. `tools/check-provenance.py`: passes (95 constants).
3. `docs/clean-room.md`: process description.
4. `git log`: phase-by-phase construction, no large unreviewed drops.
5. No binaries, ROMs, or extracted assets in the repo
   (`demo/bin/*.so` are our compiled Rust code).

## Similarity audit

`tools/similarity-audit.py` scans for:
- Long identifier matches against a blocklist of known proprietary symbols
  (none expected; the blocklist documents what we avoid).
- Accidental inclusion of binary blobs or base64-encoded assets.

Run: `python3 tools/similarity-audit.py`
