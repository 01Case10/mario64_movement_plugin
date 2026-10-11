#!/usr/bin/env python3
"""Generate crates/stepkit-core/src/trig_tables.rs from the decomp's
include/trig_tables.inc.c (plan §4.1 FD-03).

The decomp is built with -DAVOID_UB=1 (see oracle/build.sh), under which
math_util.h defines gCosineTable as (gSineTable + 0x400): the single flat
array in trig_tables.inc.c holds 1024 sine values followed by 4096 cosine
values. We mirror that layout exactly:

    SINE_TABLE[0..1024]      = sin(i * 16 units), i.e. sin(i * 2π/4096)
    SINE_TABLE[1024..5120]   = cos(i * 16 units)

with sins(x) = SINE_TABLE[((u16)x) >> 4] and
     coss(x) = SINE_TABLE[0x400 + (((u16)x) >> 4)]
(decomp: src/engine/math_util.h:24-28).

gArctanTable (0x401 s16 entries) is copied verbatim for atan2_lookup/
atan2s (decomp: src/engine/math_util.c:698-713).

The generated file is checked in so the crate builds without the decomp
checkout (oracle/sm64/ is git-ignored). Re-run this script after changing
the pinned decomp commit (oracle/DECOMP_COMMIT); a unit test in trig.rs
re-validates the checked-in tables against the decomp source whenever the
checkout is present.
"""

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SRC = REPO / "oracle" / "sm64" / "include" / "trig_tables.inc.c"
OUT = REPO / "crates" / "stepkit-core" / "src" / "trig_tables.rs"

EXPECTED_SIN_COS = 1024 + 4096
EXPECTED_ARCTAN = 0x401


def parse_float_tokens(text: str) -> list[str]:
    """Return the raw C float literal tokens (suffix stripped), in order.

    The original literal text is preserved verbatim so the Rust compiler
    parses the exact same decimal value the C compiler saw (plan §5 rule 2:
    mirror C's arithmetic conversions).
    """
    toks: list[str] = []
    for tok in re.split(r"[,\s]+", text.strip()):
        # Keep only float-literal-looking tokens; the table embeds `//`
        # comments ("// cosine", "// negative sine", ...) between values.
        if not re.fullmatch(r"[+-]?[0-9][0-9.eE+-]*[fF]?", tok):
            continue
        toks.append(tok.rstrip("fF"))
    return toks


def main() -> None:
    if not SRC.is_file():
        sys.exit(f"decomp source not found: {SRC} (clone per oracle/README.md)")
    src = SRC.read_text()

    # Under -DAVOID_UB the file declares ONE flat array: the gSineTable
    # initializer runs from its declaration to the final `};` before
    # gArctanTable (the `#ifndef AVOID_UB` / `};` /
    # `f32 gCosineTable[0x1000] = {` / `#endif` lines in between vanish).
    m = re.search(r"f32 gSineTable\[\] = \{(.*?)s16 gArctanTable", src, re.S)
    if not m:
        sys.exit("could not find gSineTable initializer")
    body_lines = [
        ln
        for ln in m.group(1).splitlines()
        if not ln.lstrip().startswith("#")
        and ln.strip() not in ("};", "f32 gCosineTable[0x1000] = {")
    ]
    floats = parse_float_tokens("\n".join(body_lines))
    if len(floats) != EXPECTED_SIN_COS:
        sys.exit(f"expected {EXPECTED_SIN_COS} sin/cos values, got {len(floats)}")

    m = re.search(r"s16 gArctanTable\[0x401\] = \{(.*?)\};", src, re.S)
    if not m:
        sys.exit("could not find gArctanTable initializer")
    atan: list[int] = []
    for tok in re.split(r"[,\s]+", m.group(1).strip()):
        if tok:
            atan.append(int(tok, 16))
    if len(atan) != EXPECTED_ARCTAN:
        sys.exit(f"expected {EXPECTED_ARCTAN} arctan values, got {len(atan)}")

    lines = [
        "//! Trig tables ported from the decomp (`include/trig_tables.inc.c`).",
        "//!",
        "//! DO NOT EDIT BY HAND. Regenerate with `tools/gen_trig_tables.py`",
        "//! (decomp commit pinned in `oracle/DECOMP_COMMIT`).",
        "//!",
        "//! Layout mirrors the decomp's `-DAVOID_UB=1` build: one flat array",
        "//! holding 1024 sine values followed by 4096 cosine values",
        "//! (decomp: src/engine/math_util.h:24-28):",
        "//! `sins(x) = SINE_TABLE[((x as u16) >> 4)]`,",
        "//! `coss(x) = SINE_TABLE[0x400 + ((x as u16) >> 4)]`.",
        "//! No interpolation — angles are quantised to 16-unit steps, exactly",
        "//! like the decomp. See `crate::trig`.",
        "",
        "/// sin/cos table: 1024 sine entries then 4096 cosine entries.",
        "/// decomp: include/trig_tables.inc.c (`gSineTable`, AVOID_UB layout).",
        "pub const SINE_TABLE: [f32; 5120] = [",
    ]
    for i in range(0, len(floats), 8):
        chunk = floats[i : i + 8]
        # Bare literals: the `[f32; N]` type ascribes f32, which is exactly
        # what the C `f` suffix means (plan §5 rule 2).
        lines.append("    " + ", ".join(chunk) + ",")
    lines += [
        "];",
        "",
        "/// Arctangent lookup table for `atan2s` (1025 entries).",
        "/// decomp: include/trig_tables.inc.c (`gArctanTable`).",
        "pub const ARCTAN_TABLE: [i16; 1025] = [",
    ]
    for i in range(0, len(atan), 16):
        chunk = atan[i : i + 16]
        lines.append("    " + ", ".join(f"0x{v:04X}" for v in chunk) + ",")
    lines += [" ];", ""]
    OUT.write_text("\n".join(lines) + "\n")
    print(f"wrote {OUT} ({len(floats)} sin/cos, {len(atan)} arctan)")


if __name__ == "__main__":
    main()
