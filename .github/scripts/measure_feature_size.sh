#!/usr/bin/env bash
# Measure flash (.text) for size-harness under minimal / lighting / full feature sets.
#
# Usage:
#   ./.github/scripts/measure_feature_size.sh           # print table
#   ./.github/scripts/measure_feature_size.sh --check   # also fail if minimal .text exceeds budget
#
# Budget is intentionally generous vs today's ~63 KiB to catch large accidental
# regressions without being brittle to minor codegen noise.
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

TARGET="${SIZE_HARNESS_TARGET:-thumbv7em-none-eabihf}"
# ~63 KiB today; fail if minimal climbs above 80 KiB.
MINIMAL_TEXT_BUDGET="${MINIMAL_TEXT_BUDGET:-81920}"
OUT_DIR="${SIZE_HARNESS_OUT_DIR:-target/${TARGET}/release}"
# Scoped to cargo invocations only — do not export into the parent shell.
RUSTFLAGS_FOR_HARNESS="${RUSTFLAGS:--C link-arg=-Tlink.x}"

need_size() {
  if command -v arm-none-eabi-size >/dev/null 2>&1; then
    echo "arm-none-eabi-size"
  elif command -v llvm-size >/dev/null 2>&1; then
    echo "llvm-size"
  elif command -v rust-size >/dev/null 2>&1; then
    echo "rust-size"
  else
    echo "pre-commit/measure: need arm-none-eabi-size, llvm-size, or rust-size" >&2
    exit 1
  fi
}

SIZE_BIN="$(need_size)"

text_size() {
  local elf="$1"
  # portable: second line of size output, first column
  "$SIZE_BIN" "$elf" | awk 'NR==2 {print $1}'
}

build_variant() {
  local name="$1"
  shift
  echo "measure: building size-harness ($name) ..." >&2
  RUSTFLAGS="$RUSTFLAGS_FOR_HARNESS" cargo build -p size-harness --release --target "$TARGET" "$@"
  local elf="${OUT_DIR}/size-harness"
  if [[ ! -f "$elf" ]]; then
    echo "measure: missing elf $elf after build ($name)" >&2
    exit 1
  fi
  cp "$elf" "${OUT_DIR}/size-harness-${name}"
  text_size "${OUT_DIR}/size-harness-${name}"
}

rustup target add "$TARGET" >/dev/null 2>&1 || true

minimal_text="$(build_variant minimal --no-default-features --features row_width_320,depth-u16)"
lighting_text="$(build_variant lighting --no-default-features --features row_width_320,depth-u16,lighting)"
full_text="$(build_variant full --no-default-features --features row_width_320,depth-u16,full)"

printf '\nFeature set flash (.text) — target %s\n' "$TARGET"
printf '%-12s %10s %10s\n' "variant" "text_bytes" "delta_min"
printf '%-12s %10s %10s\n' "--------" "----------" "---------"
printf '%-12s %10d %10s\n' "minimal" "$minimal_text" "-"
printf '%-12s %10d %10d\n' "lighting" "$lighting_text" "$((lighting_text - minimal_text))"
printf '%-12s %10d %10d\n' "full" "$full_text" "$((full_text - minimal_text))"

if [[ "${1:-}" == "--check" ]]; then
  if (( minimal_text > MINIMAL_TEXT_BUDGET )); then
    echo "measure: FAIL minimal .text ${minimal_text} exceeds budget ${MINIMAL_TEXT_BUDGET}" >&2
    exit 1
  fi
  echo "measure: OK minimal .text ${minimal_text} <= budget ${MINIMAL_TEXT_BUDGET}"
fi
