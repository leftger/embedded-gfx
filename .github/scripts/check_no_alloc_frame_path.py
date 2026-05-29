#!/usr/bin/env python3
"""Guard against obvious heap allocation calls in frame-path functions."""

from __future__ import annotations

import re
import sys
from pathlib import Path

FRAME_FUNCS = (
    "render",
    "record_render_commands",
    "record_render_commands_with_telemetry",
    "execute_recorded_frame",
    "execute_recorded_frame_with_telemetry",
)

FORBIDDEN_PATTERNS = (
    r"(?<!heapless::)\bVec::new\(",
    r"\bString::new\(",
    r"\bboxed::",
    r"\bBox<",
    r"\bformat!\(",
)


def extract_block(content: str, fn_name: str) -> str:
    match = re.search(rf"(?m)^\s*pub fn\s+{re.escape(fn_name)}(?:<|\s*\()", content)
    if not match:
        return ""
    idx = match.start()
    start = content.rfind("\n", 0, idx) + 1
    brace = content.find("{", match.end())
    if brace < 0:
        return ""
    depth = 0
    for i in range(brace, len(content)):
        ch = content[i]
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                return content[start : i + 1]
    return ""


def main() -> int:
    lib = Path("src/lib.rs")
    text = lib.read_text(encoding="utf-8")
    failures: list[str] = []
    for fn_name in FRAME_FUNCS:
        block = extract_block(text, fn_name)
        if not block:
            failures.append(f"missing frame function: {fn_name}")
            continue
        for pat in FORBIDDEN_PATTERNS:
            if re.search(pat, block):
                failures.append(f"{fn_name}: matches forbidden pattern {pat}")

    if failures:
        for line in failures:
            print(f"::error::{line}", file=sys.stderr)
        return 1
    print("no-alloc frame path guard passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
