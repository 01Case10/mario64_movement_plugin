#!/usr/bin/env python3
"""Provenance lint: every behavior-shaping constant in stepkit-core must have
a provenance entry in spec/constants.toml, referenced from its doc comment
as `spec: <section>.<key>`.

Checks:
1. Every [section.key] in spec/constants.toml carries value, unit, source,
   evidence, confidence, reviewed_by, date.
2. Every `pub const NAME: f32` / `: u32` / `: i32` in crates/stepkit-core/src
   has a doc comment on the lines above it containing `spec: <key>`, and the
   key exists in constants.toml.

Exit 0 when clean, 1 with a report otherwise.
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TOML = ROOT / "spec" / "constants.toml"
CORE = ROOT / "crates" / "stepkit-core" / "src"

REQUIRED_FIELDS = ["value", "unit", "source", "evidence", "confidence", "reviewed_by", "date"]


def parse_toml_sections(text):
    """Minimal [section.key] parser good enough for constants.toml."""
    sections = {}
    current = None
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        m = re.match(r"\[(.+)\]", line)
        if m:
            current = m.group(1)
            sections[current] = {}
        elif current and "=" in line:
            key = line.split("=", 1)[0].strip()
            sections[current][key] = True
    return sections


def main():
    errors = []
    sections = parse_toml_sections(TOML.read_text())
    for name, fields in sections.items():
        if name == "meta":
            continue
        for req in REQUIRED_FIELDS:
            if req not in fields:
                errors.append(f"{TOML.name}: [{name}] missing field `{req}`")
        if fields.get("confidence") and False:
            pass

    # Check confidence values are known.
    text = TOML.read_text()
    for m in re.finditer(r'confidence\s*=\s*"([^"]+)"', text):
        if m.group(1) not in ("low", "medium", "high"):
            errors.append(f"{TOML.name}: bad confidence `{m.group(1)}`")

    # Scan core constants for spec: references.
    const_re = re.compile(r"pub const (\w+)\s*:\s*(f32|u32|i32|u16|i16)\s*=")
    specref_re = re.compile(r"spec:\s*([\w.]+)")
    for path in sorted(CORE.rglob("*.rs")):
        lines = path.read_text().splitlines()
        for i, line in enumerate(lines):
            m = const_re.search(line)
            if not m:
                continue
            name = m.group(1)
            if name in ("CRATE_VERSION", "FULL_TURN_UNITS", "TABLE_BITS", "TABLE_SIZE",
                        "QUARTER_STEPS", "ZERO", "QUARTER_TURN", "HALF_TURN", "CUSTOM_BASE",
                        "A", "B", "Z", "R", "START",
                        "IDLE", "WALK", "TURN", "BRAKE", "DECEL", "LAND",
                        "JUMP", "FREEFALL", "DOUBLE_JUMP", "TRIPLE_JUMP",
                        "BACKFLIP", "SIDE_FLIP", "LONG_JUMP", "DIVE",
                        "GROUND_POUND", "CROUCH", "CRAWL",
                        "WALL_KICK", "LEDGE_GRAB", "AIR_KNOCKBACK",
                        "BUTT_SLIDE", "WATER_PLUNGE", "SWIMMING",
                        "SLIDE_KICK", "ROLLOUT", "STEEP_JUMP", "AIR_HIT_WALL",
                        "SOFT_BONK", "DIVE_SLIDE", "CROUCH_SLIDE",
                        "STOMACH_SLIDE", "SLIDE_KICK_SLIDE", "FINISH_TURN"):
                continue  # structural (anim slot ids, angle units) or button bits
            window = "\n".join(lines[max(0, i - 4):i])
            sm = specref_re.search(window)
            if not sm:
                errors.append(f"{path.name}:{i+1}: const `{name}` has no `spec: <key>` doc reference")
            elif sm.group(1) not in sections:
                errors.append(f"{path.name}:{i+1}: const `{name}` cites unknown spec key `{sm.group(1)}`")

    # params.rs documents each field with `spec: <key>`; those keys must exist.
    params_text = (ROOT / "crates" / "stepkit-core" / "src" / "params.rs").read_text()
    for m in specref_re.finditer(params_text):
        key = m.group(1)
        if key.endswith("."):
            continue  # section wildcard in a comment, not a key reference
        if key not in sections:
            errors.append(f"params.rs: unknown spec key `{key}`")

    if errors:
        print("provenance lint FAILED:")
        for e in errors:
            print("  -", e)
        return 1
    print(f"provenance lint OK ({len([s for s in sections if s != 'meta'])} constants tracked)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
