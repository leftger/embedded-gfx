#!/usr/bin/env python3
"""Parse CI_TELEMETRY output and enforce command-count budgets."""

from __future__ import annotations

import argparse
import os
import re
import sys
from pathlib import Path

LINE_RE = re.compile(r"(CI_TELEMETRY(?:_STRESS|_FAILSOFT)?)\s+(.*)")
ENV_MAX_RECORD = "TELEMETRY_MAX_RECORD_DRAW_COMMANDS"
ENV_MAX_EXEC_TOTAL = "TELEMETRY_MAX_EXECUTE_COMMANDS_TOTAL"
ENV_MAX_EXEC_DRAW = "TELEMETRY_MAX_EXECUTE_DRAW_COMMANDS"
ENV_STRESS_MAX_RECORD = "TELEMETRY_STRESS_MAX_RECORD_DRAW_COMMANDS"
ENV_STRESS_MAX_EXEC_TOTAL = "TELEMETRY_STRESS_MAX_EXECUTE_COMMANDS_TOTAL"
ENV_STRESS_MAX_EXEC_DRAW = "TELEMETRY_STRESS_MAX_EXECUTE_DRAW_COMMANDS"
ENV_FAILSOFT_MAX_RECORD = "TELEMETRY_FAILSOFT_MAX_RECORD_DRAW_COMMANDS"
ENV_FAILSOFT_MAX_EXEC_TOTAL = "TELEMETRY_FAILSOFT_MAX_EXECUTE_COMMANDS_TOTAL"
ENV_FAILSOFT_MAX_EXEC_DRAW = "TELEMETRY_FAILSOFT_MAX_EXECUTE_DRAW_COMMANDS"
ENV_FAILSOFT_EXPECTED_BUDGET_KIND = "TELEMETRY_FAILSOFT_EXPECTED_BUDGET_KIND"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Validate telemetry command-count budgets from test output."
    )
    parser.add_argument(
        "--input",
        default="-",
        help="Input file path, or '-' to read from stdin (default).",
    )
    parser.add_argument(
        "--max-record-draw-commands",
        type=int,
        default=None,
        help="Maximum allowed record_draw_commands.",
    )
    parser.add_argument(
        "--max-execute-commands-total",
        type=int,
        default=None,
        help="Maximum allowed execute_commands_total.",
    )
    parser.add_argument(
        "--max-execute-draw-commands",
        type=int,
        default=None,
        help="Maximum allowed execute_draw_commands.",
    )
    return parser.parse_args()


def resolve_limit(cli_value: int | None, env_name: str, default: int) -> int:
    # CLI explicitly wins over env for predictable one-off overrides.
    if cli_value is not None:
        return cli_value
    raw = os.getenv(env_name)
    if raw is None:
        return default
    try:
        return int(raw)
    except ValueError as err:
        raise ValueError(f"Invalid integer in {env_name}: {raw}") from err


def read_input(path: str) -> str:
    if path == "-":
        return sys.stdin.read()
    return Path(path).read_text(encoding="utf-8")


def extract_metrics(text: str) -> list[tuple[str, dict[str, str]]]:
    matches = LINE_RE.findall(text)
    if not matches:
        raise ValueError(
            "No CI_TELEMETRY, CI_TELEMETRY_STRESS, or CI_TELEMETRY_FAILSOFT line found in input."
        )

    parsed: list[tuple[str, dict[str, str]]] = []
    required = {
        "record_draw_commands",
        "execute_commands_total",
        "execute_draw_commands",
        "fallback_used",
    }
    for idx, (kind, line) in enumerate(matches):
        data: dict[str, str] = {}
        for token in line.split():
            if "=" not in token:
                continue
            key, value = token.split("=", 1)
            data[key] = value
        missing = sorted(required.difference(data.keys()))
        if missing:
            raise ValueError(
                f"Missing metrics in {kind} line #{idx + 1}: {', '.join(missing)}"
            )
        parsed.append((kind, data))
    return parsed


def main() -> int:
    args = parse_args()
    try:
        max_record_draw = resolve_limit(args.max_record_draw_commands, ENV_MAX_RECORD, 3)
        max_execute_total = resolve_limit(args.max_execute_commands_total, ENV_MAX_EXEC_TOTAL, 4)
        max_execute_draw = resolve_limit(args.max_execute_draw_commands, ENV_MAX_EXEC_DRAW, 3)
        stress_max_record_draw = resolve_limit(None, ENV_STRESS_MAX_RECORD, 24)
        stress_max_execute_total = resolve_limit(None, ENV_STRESS_MAX_EXEC_TOTAL, 25)
        stress_max_execute_draw = resolve_limit(None, ENV_STRESS_MAX_EXEC_DRAW, 24)
        failsoft_max_record_draw = resolve_limit(None, ENV_FAILSOFT_MAX_RECORD, 1)
        failsoft_max_execute_total = resolve_limit(None, ENV_FAILSOFT_MAX_EXEC_TOTAL, 2)
        failsoft_max_execute_draw = resolve_limit(None, ENV_FAILSOFT_MAX_EXEC_DRAW, 1)
        failsoft_expected_budget_kind = os.getenv(
            ENV_FAILSOFT_EXPECTED_BUDGET_KIND, "MeshesPerFrame"
        )
        text = read_input(args.input)
        snapshots = extract_metrics(text)
    except ValueError as err:
        print(f"::error::{err}", file=sys.stderr)
        return 2

    failures: list[str] = []
    for idx, (kind, raw_metrics) in enumerate(snapshots):
        try:
            metrics = {
                "record_draw_commands": int(raw_metrics["record_draw_commands"]),
                "execute_commands_total": int(raw_metrics["execute_commands_total"]),
                "execute_draw_commands": int(raw_metrics["execute_draw_commands"]),
                "fallback_used": int(raw_metrics["fallback_used"]),
            }
        except (KeyError, ValueError) as err:
            print(
                f"::error::Invalid numeric telemetry fields in snapshot #{idx + 1}: {err}",
                file=sys.stderr,
            )
            return 2
        if kind == "CI_TELEMETRY_STRESS":
            limits = (
                stress_max_record_draw,
                stress_max_execute_total,
                stress_max_execute_draw,
            )
            expected_fallback = 0
        elif kind == "CI_TELEMETRY_FAILSOFT":
            limits = (
                failsoft_max_record_draw,
                failsoft_max_execute_total,
                failsoft_max_execute_draw,
            )
            expected_fallback = 1
        else:
            limits = (max_record_draw, max_execute_total, max_execute_draw)
            expected_fallback = 0
        checks = [
            ("record_draw_commands", metrics["record_draw_commands"], limits[0]),
            ("execute_commands_total", metrics["execute_commands_total"], limits[1]),
            ("execute_draw_commands", metrics["execute_draw_commands"], limits[2]),
        ]
        print(
            f"{kind} snapshot #{idx + 1}: "
            + ", ".join(f"{name}={value}" for name, value, _ in checks)
            + f", fallback_used={metrics['fallback_used']}"
        )
        for name, value, limit in checks:
            if value > limit:
                failures.append(
                    f"snapshot #{idx + 1}: {name}={value} exceeds limit {limit}"
                )
        if metrics["fallback_used"] != expected_fallback:
            failures.append(
                f"snapshot #{idx + 1}: fallback_used={metrics['fallback_used']} expected {expected_fallback} for {kind}"
            )
        if kind == "CI_TELEMETRY_FAILSOFT":
            actual_kind = raw_metrics.get("primary_budget_kind", "")
            if actual_kind != failsoft_expected_budget_kind:
                failures.append(
                    f"snapshot #{idx + 1}: primary_budget_kind={actual_kind or '<missing>'} expected {failsoft_expected_budget_kind}"
                )

    if failures:
        for item in failures:
            print(f"::error::{item}", file=sys.stderr)
        return 1

    print(f"Telemetry budget check passed for {len(snapshots)} snapshot(s).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
