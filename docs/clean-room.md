# Clean-room process (retired)

Up to 2026-10-10, StepKit was built under a clean-room policy: no SM64
decompilation source, libsm64, ROM disassembly, or extracted data was
read or used. Gameplay numbers came from public behavioral
documentation (the community wiki), our own simulation traces, and
marked working assumptions (`(verify)`), tracked in
`spec/constants.toml`.

On 2026-10-11 the project owner formally adopted a **decomp-derived
policy**: movement behavior is now ported from the SM64 decompilation
(`n64decomp/sm64`) and verified against a native oracle built from the
decomp's own movement code. The clean-room policy no longer applies.

The old process records (`spec/constants.toml`, `tools/check-provenance.py`,
`tools/similarity-audit.py`) are kept as deprecated reference tooling
and are no longer release-gating. `spec/constants.toml` will be
removed once its values are superseded by the ported code. See
`docs/decomp-parity-plan.md` and `docs/parity-log.md`.
