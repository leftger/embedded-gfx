#!/usr/bin/env python3
"""Validate HARDWARE_SMOKE log lines from smoke workflow."""

from __future__ import annotations

import re
import sys
from pathlib import Path

LINE_RE = re.compile(
    r"HARDWARE_SMOKE\s+board=(?P<board>\S+)\s+profile=(?P<profile>\S+)\s+stage=(?P<stage>\S+)\s+status=(?P<status>\S+)"
)

REQUIRED = {
    ("build", "pass"),
    ("telemetry_test", "pass"),
    ("telemetry_guard", "pass"),
    ("smoke", "complete"),
}


def main() -> int:
    path = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("hardware-smoke.log")
    if not path.exists():
        print(f"::error::missing smoke log: {path}", file=sys.stderr)
        return 2
    text = path.read_text(encoding="utf-8")
    matches = [m.groupdict() for m in LINE_RE.finditer(text)]
    if not matches:
        print("::error::no HARDWARE_SMOKE lines found", file=sys.stderr)
        return 1
    seen = {(m["stage"], m["status"]) for m in matches}
    missing = REQUIRED.difference(seen)
    if missing:
        for stage, status in sorted(missing):
            print(
                f"::error::missing HARDWARE_SMOKE stage={stage} status={status}",
                file=sys.stderr,
            )
        return 1
    print(f"hardware smoke log check passed ({len(matches)} markers)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
