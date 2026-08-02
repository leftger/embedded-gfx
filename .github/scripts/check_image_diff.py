#!/usr/bin/env python3
"""
Golden-image visual regression comparison tool for embedded-3dgfx.
Compares PNG images in --rendered-dir against baseline PNG images in --golden-dir.
"""

from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path

try:
    from PIL import Image
except ImportError:
    Image = None


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Validate rendered PNGs against golden images.")
    parser.add_argument(
        "--golden-dir",
        default="tests/golden_images",
        help="Path to directory containing reference golden PNG images.",
    )
    parser.add_argument(
        "--rendered-dir",
        default="target/rendered_images",
        help="Path to directory containing newly rendered PNG images.",
    )
    parser.add_argument(
        "--max-channel-delta",
        type=int,
        default=2,
        help="Maximum allowed difference per color channel (0-255).",
    )
    parser.add_argument(
        "--max-differing-pixels",
        type=int,
        default=5,
        help="Maximum allowed number of differing pixels.",
    )
    return parser.parse_args()


def compare_images_pil(
    golden_path: Path, rendered_path: Path, max_channel_delta: int, max_differing_pixels: int
) -> tuple[bool, str]:
    gold = Image.open(golden_path).convert("RGB")
    rend = Image.open(rendered_path).convert("RGB")

    if gold.size != rend.size:
        return False, f"Size mismatch: golden {gold.size} vs rendered {rend.size}"

    get_pixels = getattr(gold, "get_flattened_data", gold.getdata)
    gold_pixels = list(get_pixels())
    rend_pixels = list(getattr(rend, "get_flattened_data", rend.getdata)())

    differing_pixels = 0
    max_delta_seen = 0

    for idx, ((r1, g1, b1), (r2, g2, b2)) in enumerate(zip(gold_pixels, rend_pixels)):
        dr = abs(r1 - r2)
        dg = abs(g1 - g2)
        db = abs(b1 - b2)
        delta = max(dr, dg, db)

        if delta > max_channel_delta:
            differing_pixels += 1
            if delta > max_delta_seen:
                max_delta_seen = delta

    if differing_pixels > max_differing_pixels:
        return (
            False,
            f"Failed: {differing_pixels} pixels differ beyond channel delta {max_channel_delta} (max delta seen: {max_delta_seen})",
        )

    return True, f"Passed ({differing_pixels} minor pixel differences, max delta: {max_delta_seen})"


def compare_images(
    golden_path: Path, rendered_path: Path, max_channel_delta: int, max_differing_pixels: int
) -> tuple[bool, str]:
    if Image is not None:
        return compare_images_pil(golden_path, rendered_path, max_channel_delta, max_differing_pixels)
    
    gold_bytes = golden_path.read_bytes()
    rend_bytes = rendered_path.read_bytes()
    if gold_bytes == rend_bytes:
        return True, "Passed (exact byte match)"
    return False, f"Byte mismatch ({len(gold_bytes)} vs {len(rend_bytes)} bytes)"


def main() -> int:
    args = parse_args()
    golden_dir = Path(args.golden_dir)
    rendered_dir = Path(args.rendered_dir)

    if not golden_dir.exists():
        print(f"Golden directory does not exist: {golden_dir}")
        return 1

    if not rendered_dir.exists():
        print(f"Rendered directory does not exist: {rendered_dir}")
        return 1

    golden_files = list(golden_dir.glob("*.png"))
    if not golden_files:
        print(f"No golden PNG images found in {golden_dir}")
        return 1

    failures = 0
    passed = 0

    print(f"Comparing {len(golden_files)} golden image(s)...")
    for golden_path in sorted(golden_files):
        rendered_path = rendered_dir / golden_path.name
        if not rendered_path.exists():
            print(f"FAIL {golden_path.name}: Rendered image missing in {rendered_dir}")
            failures += 1
            continue

        ok, msg = compare_images(
            golden_path, rendered_path, args.max_channel_delta, args.max_differing_pixels
        )
        if ok:
            print(f"OK   {golden_path.name}: {msg}")
            passed += 1
        else:
            print(f"FAIL {golden_path.name}: {msg}")
            failures += 1

    print(f"\nSummary: {passed} passed, {failures} failed out of {len(golden_files)} golden images.")
    return 1 if failures > 0 else 0


if __name__ == "__main__":
    sys.exit(main())
