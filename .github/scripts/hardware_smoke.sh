#!/usr/bin/env bash
set -euo pipefail

BOARD="${1:-unknown}"
PROFILE="${2:-m4}"
ROW_FEATURE="${3:-row_width_240}"

log_stage() {
  local stage="$1"
  local status="$2"
  echo "HARDWARE_SMOKE board=${BOARD} profile=${PROFILE} stage=${stage} status=${status}"
}

log_stage build start
EMBEDDED_3DGFX_CAPS="${PROFILE}" \
  cargo check --lib --no-default-features --features "${ROW_FEATURE}"
log_stage build pass

log_stage telemetry_test start
EMBEDDED_3DGFX_CAPS="${PROFILE}" \
  cargo test --test integration_tests test_ci_telemetry_snapshot_ -- --nocapture \
  2>&1 | tee hardware-smoke.log
log_stage telemetry_test pass

log_stage telemetry_guard start
.github/scripts/check_telemetry_budget.py --input hardware-smoke.log
log_stage telemetry_guard pass

log_stage smoke complete
