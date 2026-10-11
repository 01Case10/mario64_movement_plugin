#!/usr/bin/env bash
# Parity gate: diff the plugin against the native decomp oracle across all
# scenarios. The diff table is always printed; the script exits nonzero iff
# the diff reports any failure. During Phase 2 the diff is expected to be RED
# while the plugin is being rewritten to match the oracle (§6 items 3-5).
#
# Usage: ./tools/parity-gate.sh
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

echo "=== build cli ==="
cargo build -p stepkit-cli

if [ ! -x oracle/build/oracle ]; then
  echo "=== build oracle ==="
  ./oracle/build.sh
fi

echo "=== oracle diff --all ==="
# `oracle diff` exits nonzero on any failure; always print the full table
# first, then summarize and propagate its exit code.
rc=0
./target/debug/stepkit oracle diff --all || rc=$?
echo ""
if [ "$rc" -eq 0 ]; then
  echo "PARITY GATE: GREEN"
else
  echo "PARITY GATE: RED (expected until parity is reached)"
fi
exit "$rc"
