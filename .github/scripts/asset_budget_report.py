#!/usr/bin/env python3
"""Emit and enforce simple asset budget reports."""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path


def walk_sizes(root: Path, suffixes: tuple[str, ...]) -> list[tuple[str, int]]:
    rows: list[tuple[str, int]] = []
    if not root.exists():
        return rows
    for p in root.rglob("*"):
        if not p.is_file():
            continue
        if suffixes and p.suffix.lower() not in suffixes:
            continue
        rows.append((str(p), p.stat().st_size))
    return sorted(rows, key=lambda r: r[0])


def limit(name: str, default: int) -> int:
    raw = os.getenv(name)
    if raw is None:
        return default
    try:
        return int(raw)
    except ValueError as exc:
        raise ValueError(f"invalid integer in {name}: {raw}") from exc


def main() -> int:
    try:
        max_total = limit("ASSET_BUDGET_MAX_TOTAL_BYTES", 8_000_000)
        max_texture = limit("ASSET_BUDGET_MAX_TEXTURE_BYTES", 4_000_000)
        max_scene = limit("ASSET_BUDGET_MAX_SCENE_BYTES", 4_000_000)
    except ValueError as err:
        print(f"::error::{err}", file=sys.stderr)
        return 2

    texture_files = walk_sizes(
        Path("assets"),
        (".png", ".jpg", ".jpeg", ".gif", ".rgb565", ".ppm"),
    )
    scene_files = []
    for root in (Path("assets"), Path("generated-assets"), Path("scenes")):
        scene_files.extend(walk_sizes(root, (".e3dscene", ".e3dm", ".e3dt")))

    texture_total = sum(size for _, size in texture_files)
    scene_total = sum(size for _, size in scene_files)
    total = texture_total + scene_total

    report = {
        "texture_total_bytes": texture_total,
        "scene_total_bytes": scene_total,
        "total_bytes": total,
        "texture_file_count": len(texture_files),
        "scene_file_count": len(scene_files),
    }
    print("ASSET_BUDGET " + json.dumps(report, sort_keys=True))

    failed = False
    if texture_total > max_texture:
        print(
            f"::error::texture bytes {texture_total} exceed limit {max_texture}",
            file=sys.stderr,
        )
        failed = True
    if scene_total > max_scene:
        print(
            f"::error::scene bytes {scene_total} exceed limit {max_scene}",
            file=sys.stderr,
        )
        failed = True
    if total > max_total:
        print(f"::error::asset total {total} exceed limit {max_total}", file=sys.stderr)
        failed = True

    if failed:
        return 1
    print("asset budget report passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
