# Legal notes (not legal advice)

Checklist before any public release or sale:

1. IP lawyer review of the decomp-derived code, naming, and traces.
2. Resolve the licensing question for the derived portions: the
   repository claims MIT, which is questionable for code derived from
   the SM64 decompilation (see `NOTICE`, `docs/legal.md`). Owner decision.
3. Confirm golden traces (now generated from the oracle) may ship
   publicly (numbers only) or keep them private with published hashes.
4. Neutral naming everywhere user-visible: no Nintendo names or marks in
   crate names, node classes, docs, or product name.
5. Attribution note for `n64decomp/sm64` in shipped artifacts, if counsel
   advises.

History: the pre-2026-10-11 checklist assumed a clean-room process;
that policy was retired (see `docs/clean-room.md`).
