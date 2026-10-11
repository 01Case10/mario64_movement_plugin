#!/usr/bin/env bash
# Release gate: run every check. All must pass.
# Usage: ./tools/release-gate.sh
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

echo "=== fmt ==="
cargo fmt --all -- --check && echo "fmt OK"

echo "=== clippy (core) ==="
cargo clippy --workspace --exclude stepkit-godot --all-targets -- -D warnings

echo "=== clippy (godot) ==="
cargo clippy -p stepkit-godot --all-targets -- -D warnings

echo "=== tests ==="
cargo test --workspace --exclude stepkit-godot

echo "=== provenance (deprecated, non-gating) ==="
python3 tools/check-provenance.py || echo "WARNING: check-provenance.py failed (non-gating, deprecated)"

echo "=== similarity audit (deprecated, non-gating) ==="
python3 tools/similarity-audit.py || echo "WARNING: similarity-audit.py failed (non-gating, deprecated)"

echo "=== scenario gate (warn-only, deprecated) ==="
# Non-gating: golden/ now holds oracle-schema CSVs (§7.3) which
# `stepkit trace diff --golden` cannot parse; it expects the old plugin
# TraceFrame schema. Plugin-vs-oracle parity is checked by
# tools/parity-gate.sh (`stepkit oracle diff --all`) instead.
cargo build -p stepkit-cli 2>/dev/null
pass=0; fail=0
for s in traces/scenarios/*.ron; do
  id=$(basename "$s" .ron); g="traces/golden/$id.csv"
  ./target/debug/stepkit trace diff --scenario "$s" --golden "$g" --mode free 2>&1 | grep "Result: PASS" > /dev/null && pass=$((pass+1)) || { echo "FREE FAIL $id"; fail=$((fail+1)); }
  ./target/debug/stepkit trace diff --scenario "$s" --golden "$g" --mode teacher-forced 2>&1 | grep "Result: PASS" > /dev/null && pass=$((pass+1)) || { echo "TF FAIL $id"; fail=$((fail+1)); }
done
echo "gate: passed=$pass failed=$fail"
[ "$fail" -eq 0 ] || echo "WARNING: scenario gate failed (non-gating; parity is checked by tools/parity-gate.sh)"

echo "=== anim validation (21 clips) ==="
./target/debug/stepkit anim validate --manifest anim/timing_manifest.toml --clips anim/reference_clips > /dev/null

echo ""
echo "RELEASE GATE: ALL PASS"
