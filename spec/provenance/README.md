# Provenance index

Evidence for every constant and rule in `../constants.toml`. Each entry
records the source type (wiki page + revision, trace ID, or probe ID),
retrieval date, and reviewer.

Conventions:

- `wiki:<PageTitle>@<revision>` -- public wiki page and revision id.
- `anchor:<id>` -- a CSV in `traces/public-anchors/`.
- `trace:<scenario-id>` -- a golden scenario + trace.
- `probe:<id>` -- a black-box probe run against the private oracle.
- Confidence: `low` (working assumption, marked (verify) in specs),
  `medium` (one independent source), `high` (two sources agree or a
  golden scenario passes at tiers A and B).
