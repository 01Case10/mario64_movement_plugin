# Traces, scenarios, and the data pipeline

## Layout

- `scenarios/` -- 20 scenario files (RON): our own geometry, start states,
  and input sequences. Ground sweeps, slopes, ledges, walls, and the jump
  catalog live here.
- `golden/` -- numeric golden traces (CSV + JSON sidecar). Populated by
  `tools/regen-golden` once the core implements the actions.
- `public-anchors/` -- small CSVs transcribed from public wiki pages, each
  citing page title and revision. The lowest-risk test data; built first.
- `baselines/` -- replay baselines (`replays.toml`) for L3.

## Pipeline status

Phase 1 delivered the machinery; the core is still a stub, so:

- `stepkit trace run --scenario S --out T.csv` executes a scenario through
  the core and writes the trace. All 20 scenarios round-trip end to end
  (run -> CSV -> diff) in both free-running and teacher-forced modes
  (40/40 checks pass; the diff was also verified to catch an injected
  divergence).
- Golden traces in `golden/` will be recorded from the Phase 2+ core and
  reviewed before commit. Nothing regenerates automatically.
- The private oracle runner (needs your ROM + libsm64 build) lives outside
  this repo under `oracle/` (gitignored). Only scenario definitions and
  numeric traces cross into the repo.

## Commands

```sh
stepkit trace diff --scenario traces/scenarios/walk_accel_mag80_flat.ron \
                   --golden traces/golden/walk_accel_mag80_flat.csv --mode free
stepkit trace diff --all --mode teacher-forced --report out/divergence.md  # planned
stepkit trace summarize --golden traces/golden --out spec/evidence/durations.csv
stepkit trace pack traces/golden --out traces/golden.pack
stepkit trace import-m64 inputs.m64 --frames 0..1800 --out traces/scenarios/replay_inputs.ron
```
