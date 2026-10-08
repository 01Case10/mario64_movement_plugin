#!/usr/bin/env python3
"""Similarity audit: scan for accidental proprietary contamination.

Checks:
1. No blocklisted proprietary identifiers (decomp function names etc.).
2. No large binary blobs or base64-encoded assets outside demo/bin.
3. No ROM file extensions.

The blocklist documents symbols we must NOT have; a match is a failure.
This is a tripwire, not proof of cleanliness (see docs/clean-room.md).
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Proprietary-style identifiers that must never appear in our tree.
# (These are examples of what clean-room avoids; the list is illustrative.)
BLOCKLIST = [
    r"\bmario_action\b",
    r"\bexec_mario_action\b",
    r"\bfunc_802\w+",
    r"\bD_803\w+",
    r"\bgMarioState\b",
    r"\bSM64_LIB\b",
]

BINARY_EXTS = {".z64", ".n64", ".v64", ".rom", ".iso", ".wad"}
ALLOW_BIN_DIRS = {"demo/bin", "target"}

def main() -> int:
    failures = []
    for path in ROOT.rglob("*"):
        if not path.is_file() or ".git" in path.parts or ".godot" in path.parts:
            continue
        rel = path.relative_to(ROOT)
        # Binary extension check.
        if path.suffix.lower() in BINARY_EXTS:
            failures.append(f"ROM-like file: {rel}")
            continue
        # Skip compiled artifacts we produce ourselves.
        if any(str(rel).startswith(d) for d in ALLOW_BIN_DIRS):
            continue
        # Text scan.
        try:
            text = path.read_text(encoding="utf-8", errors="strict")
        except (UnicodeDecodeError, OSError):
            # Non-UTF8 file outside allowlist: flag it.
            if path.suffix.lower() not in {".png", ".jpg", ".jpeg", ".tres", ".godot"}:
                failures.append(f"non-text blob: {rel}")
            continue
        for pat in BLOCKLIST:
            if re.search(pat, text):
                failures.append(f"blocklisted symbol {pat} in {rel}")
    if failures:
        print("similarity audit FAILED:")
        for f in failures:
            print(f"  - {f}")
        return 1
    print("similarity audit OK (no blocklisted symbols, no ROM blobs)")
    return 0

if __name__ == "__main__":
    sys.exit(main())
