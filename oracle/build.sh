#!/usr/bin/env bash
# oracle/build.sh -- build the Phase 1 native oracle from the pinned decomp checkout.
#
# Usage:
#   ./oracle/build.sh            # from the repo root (or anywhere)
#   DECOMP_DIR=/path/to/sm64 ./oracle/build.sh   # reuse an existing checkout
#
# Produces:
#   oracle/build/obj/*.o        the 11 compiled decomp objects (+ stubs.o, harness.o)
#   oracle/build/anim_table.inc generated animation table (via gen_anim_table.py)
#   oracle/build/oracle         the oracle binary (scenario file -> CSV)
#   oracle/build/selftest_math  atan2s/trig self-test binary
#
# Idempotent: safe to re-run; redoes every step from scratch.

set -euo pipefail

ORACLE_DIR="$(cd "$(dirname "$0")" && pwd)"
DECOMP_DIR="${DECOMP_DIR:-$ORACLE_DIR/sm64}"
PIN="$(cat "$ORACLE_DIR/DECOMP_COMMIT")"
BUILD_DIR="$ORACLE_DIR/build"
OBJ_DIR="$BUILD_DIR/obj"

# The 11 decomp sources from plan §7.1 (order kept stable for reproducibility).
DECOMP_SOURCES=(
    src/engine/math_util.c
    src/engine/surface_collision.c
    src/engine/surface_load.c
    src/engine/graph_node.c
    src/game/mario.c
    src/game/mario_step.c
    src/game/mario_actions_stationary.c
    src/game/mario_actions_moving.c
    src/game/mario_actions_airborne.c
    src/game/mario_actions_submerged.c
    src/game/mario_actions_automatic.c
    src/game/mario_actions_object.c
)

step() { echo "==> $*"; }

obj_name() {
    # src/engine/math_util.c -> src_engine_math_util.c.o
    printf '%s' "$1" | tr '/' '_'
    printf '.o'
}

ensure_decomp() {
    step "ensuring decomp checkout at $DECOMP_DIR (pin $PIN)"
    if [ ! -d "$DECOMP_DIR/.git" ]; then
        # Fresh machine: init + shallow-fetch exactly the pinned commit.
        rm -rf "$DECOMP_DIR"
        if ! (git init -q "$DECOMP_DIR" \
                && { git -C "$DECOMP_DIR" remote add origin https://github.com/n64decomp/sm64 2>/dev/null || true; } \
                && git -C "$DECOMP_DIR" fetch --depth 1 origin "$PIN" \
                && git -C "$DECOMP_DIR" checkout -q FETCH_HEAD); then
            # Fallback: full clone, then pin to the commit.
            step "shallow fetch of pin failed; falling back to full clone"
            rm -rf "$DECOMP_DIR"
            git clone https://github.com/n64decomp/sm64 "$DECOMP_DIR"
            git -C "$DECOMP_DIR" checkout -q "$PIN"
        fi
    else
        if [ "$(git -C "$DECOMP_DIR" rev-parse HEAD)" != "$PIN" ]; then
            step "checkout is not on pin; fetching and checking out $PIN"
            { git -C "$DECOMP_DIR" remote get-url origin >/dev/null 2>&1 \
                || git -C "$DECOMP_DIR" remote add origin https://github.com/n64decomp/sm64; }
            if ! git -C "$DECOMP_DIR" fetch --depth 1 origin "$PIN"; then
                git -C "$DECOMP_DIR" fetch origin
            fi
            git -C "$DECOMP_DIR" checkout -q "$PIN"
        fi
    fi
    if [ "$(git -C "$DECOMP_DIR" rev-parse HEAD)" != "$PIN" ]; then
        echo "error: decomp checkout is not on pinned commit $PIN" >&2
        exit 1
    fi
}

compile_decomp_objects() {
    step "compiling ${#DECOMP_SOURCES[@]} decomp objects (plan §7.1 flags)"
    mkdir -p "$OBJ_DIR"
    local src
    for src in "${DECOMP_SOURCES[@]}"; do
        # EXACT flags from §7.1. Do NOT add -fwrapv, -ffast-math, -m32, -march=native.
        (cd "$DECOMP_DIR" && gcc -c -w -O1 -std=gnu99 -DVERSION_US=1 -DTARGET_N64=0 \
            -D_LANGUAGE_C=1 -DNON_MATCHING=1 -DAVOID_UB=1 \
            -ffp-contract=off -fno-stack-protector \
            -I include -I src -I . -I include/libc \
            "$src" -o "$OBJ_DIR/$(obj_name "$src")")
    done
}

gen_anim_table() {
    step "generating animation table"
    python3 "$ORACLE_DIR/gen_anim_table.py" "$DECOMP_DIR" "$BUILD_DIR/anim_table.inc"
}

compile_harness() {
    step "compiling stubs.c and harness.c"
    local cflags=( -c -w -O1 -std=gnu99 -DVERSION_US=1 -DTARGET_N64=0
                   -D_LANGUAGE_C=1 -DNON_MATCHING=1 -DAVOID_UB=1
                   -ffp-contract=off -fno-stack-protector
                   "-I$DECOMP_DIR/include" "-I$DECOMP_DIR/src" "-I$DECOMP_DIR" )
    gcc "${cflags[@]}" "$ORACLE_DIR/stubs.c" -o "$OBJ_DIR/stubs.o"
    gcc "${cflags[@]}" "$ORACLE_DIR/harness.c" -o "$OBJ_DIR/harness.o"
}

link_oracle() {
    step "linking oracle"
    local objs=()
    local src
    for src in "${DECOMP_SOURCES[@]}"; do
        objs+=( "$OBJ_DIR/$(obj_name "$src")" )
    done
    gcc "${objs[@]}" "$OBJ_DIR/stubs.o" "$OBJ_DIR/harness.o" -lm -o "$BUILD_DIR/oracle"
}

build_and_run_selftest() {
    step "building selftest_math"
    gcc -w -O1 -std=gnu99 -DVERSION_US=1 -DTARGET_N64=0 \
        -D_LANGUAGE_C=1 -DNON_MATCHING=1 -DAVOID_UB=1 \
        -ffp-contract=off -fno-stack-protector \
        "-I$DECOMP_DIR/include" "-I$DECOMP_DIR/src" "-I$DECOMP_DIR" \
        -c "$ORACLE_DIR/selftest_math.c" -o "$OBJ_DIR/selftest_math.o"
    gcc "$OBJ_DIR/selftest_math.o" "$OBJ_DIR/$(obj_name src/engine/math_util.c)" \
        -lm -o "$BUILD_DIR/selftest_math"
    step "running selftest_math"
    local out
    out="$("$BUILD_DIR/selftest_math")"
    echo "$out"
    case "$out" in
        *OK*) ;;
        *) echo "error: selftest_math did not print OK" >&2; exit 1 ;;
    esac
}

main() {
    ensure_decomp
    compile_decomp_objects
    gen_anim_table
    compile_harness
    link_oracle
    build_and_run_selftest
    step "build complete"
    echo "  decomp : $DECOMP_DIR @ $PIN"
    echo "  oracle : $BUILD_DIR/oracle"
    echo "  selftest_math : $BUILD_DIR/selftest_math"
    echo "  objects: $(ls "$OBJ_DIR"/*.o | wc -l) in $OBJ_DIR"
}

main "$@"
