# Behavior spec

One page per action, written from public behavior documentation and
black-box measurements only. See the implementation plan's "Clean-room
process" section for the roles and the reviewer's checklist.

## Rules

- Prose and tables. No source code, no function/variable names that come
  from source rather than community documentation, no pseudocode that
  mirrors a source file's structure.
- Every constant and every ordering claim has a provenance entry in
  `constants.toml`.
- Anything unverified is marked **(verify)** and blocks implementation
  until resolved.
- Constants are never inlined in code; Rust constants cite their spec key
  (`spec: <section>.<key>`), and `tools/check-provenance.py` enforces it.

## Template

See `actions/_template.md`.
