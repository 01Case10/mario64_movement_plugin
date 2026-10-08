# Clean-room process

StepKit was built without reading, referencing, or deriving from:
- SM64 decompilation source code (any version)
- libsm64 or derived libraries
- ROM disassembly or extracted data
- Nintendo models, animations, or assets

## What we used

1. **Public behavioral documentation**: the community wiki (ukikipedia)
   action pages, which describe observable behavior (speeds, heights,
   transition conditions) in prose. We cite page, revision, and retrieval
   date for every verified number.
2. **Numeric traces**: our own simulation output, compared against
   documented values (e.g., "jump reaches ~200 units").
3. **Original design**: where no public documentation exists, we author
   working assumptions, marked `(verify)`, with provisional action IDs
   (bit 30 set) that cannot collide with documented IDs.

## Quoting policy

Copyrighted sources are linked or briefly summarized (max 50 quoted words
per work across all materials). We never reproduce substantial text.

## Provenance tracking

`spec/constants.toml` records every gameplay constant with:
- value, unit
- source kind: `wiki` (page/revision/retrieved), `assumption`, or `provisional`
- evidence links (L1 test names, scenario names)
- confidence and review status

`tools/check-provenance.py` enforces that every `spec:` doc reference in
Rust code resolves to a `constants.toml` entry.

## Verification tiers

- **Tier 1 (public anchors)**: L1 tests assert documented numbers
  (e.g., double-jump vy = 52 + speed/4).
- **Tier 2 (self-consistency)**: 33 scenarios x free/teacher-forced
  comparison (66 checks), determinism tests, cross-build (aarch64).
- **Tier 3 (private oracle)**: comparison against an independent ROM-backed
  oracle. **Blocked**: requires hardware/software Joey has not provided.
  Tiers A/B exact-fidelity claims are not made.
