#!/usr/bin/env bash
set -euo pipefail

# Creates labels, milestones, and roadmap issues for embedded renderer work.
# Safe to rerun: existing items are detected and skipped/updated.

if ! command -v gh >/dev/null 2>&1; then
  echo "Error: GitHub CLI (gh) is not installed."
  exit 1
fi

if ! gh auth status >/dev/null 2>&1; then
  echo "Error: gh is not authenticated. Run: gh auth login"
  exit 1
fi

REPO="${1:-$(gh repo view --json nameWithOwner --jq '.nameWithOwner')}"
echo "Target repository: ${REPO}"

ensure_label() {
  local name="$1"
  local color="$2"
  local description="$3"

  if gh label list --repo "$REPO" --limit 500 --search "$name" --json name --jq '.[].name' | rg -F -x --quiet "$name"; then
    gh label edit "$name" --repo "$REPO" --color "$color" --description "$description" >/dev/null
    echo "Updated label: $name"
  else
    gh label create "$name" --repo "$REPO" --color "$color" --description "$description" >/dev/null
    echo "Created label: $name"
  fi
}

ensure_milestone() {
  local title="$1"
  local description="$2"

  if gh api "repos/${REPO}/milestones?state=all&per_page=100" --jq '.[].title' | rg -F -x --quiet "$title"; then
    echo "Milestone exists: $title"
  else
    gh api "repos/${REPO}/milestones" \
      --method POST \
      --field title="$title" \
      --field description="$description" >/dev/null
    echo "Created milestone: $title"
  fi
}

issue_exists() {
  local title="$1"
  gh issue list --repo "$REPO" --state all --limit 500 --search "$title in:title" --json title --jq '.[].title' \
    | rg -F -x --quiet "$title"
}

create_issue() {
  local title="$1"
  local milestone="$2"
  local labels_csv="$3"
  local body="$4"
  local -a label_args=()
  local label

  if issue_exists "$title"; then
    echo "Issue exists: $title"
    return 0
  fi

  IFS=',' read -r -a labels <<< "$labels_csv"
  for label in "${labels[@]}"; do
    label_args+=(--label "$label")
  done

  gh issue create \
    --repo "$REPO" \
    --title "$title" \
    --milestone "$milestone" \
    "${label_args[@]}" \
    --body "$body" >/dev/null

  echo "Created issue: $title"
}

echo "Ensuring labels..."
ensure_label "priority:p0" "B60205" "Highest priority"
ensure_label "priority:p1" "D93F0B" "Medium priority"
ensure_label "priority:p2" "FBCA04" "Lower priority"
ensure_label "type:feature" "1D76DB" "Feature work"
ensure_label "type:perf" "5319E7" "Performance work"
ensure_label "type:test" "0E8A16" "Testing and validation"
ensure_label "type:docs" "0052CC" "Documentation"
ensure_label "type:refactor" "6F42C1" "Refactoring"
ensure_label "area:architecture" "C5DEF5" "Architecture and boundaries"
ensure_label "area:renderer" "A2EEEF" "Render pipeline"
ensure_label "area:backend" "C2E0C6" "Hardware/backend abstraction"
ensure_label "area:memory" "F9D0C4" "Memory and allocation"
ensure_label "area:raster" "FEF2C0" "Rasterization pipeline"
ensure_label "area:profiling" "D4C5F9" "Profiling and telemetry"
ensure_label "area:assets" "FFD8B1" "Asset pipeline and formats"
ensure_label "area:ci" "BFDADC" "CI and automation"
ensure_label "area:docs" "D4E5F9" "Documentation area"
ensure_label "area:benchmark" "E4E669" "Benchmarks and performance tracking"
ensure_label "area:io" "BFD4F2" "Streaming and IO"
ensure_label "area:math" "F7C6E0" "Math and numeric pipelines"
ensure_label "platform:m3" "8B949E" "Cortex-M3 target"
ensure_label "platform:m4" "57606A" "Cortex-M4 target"
ensure_label "platform:m33" "3D444D" "Cortex-M33 target"
ensure_label "platform:m55" "24292F" "Cortex-M55 target"
ensure_label "blocked" "000000" "Blocked by dependency"
ensure_label "needs-benchmark" "F9D0C4" "Needs benchmark evidence"
ensure_label "needs-hardware-validation" "5319E7" "Needs real hardware validation"

echo "Ensuring milestones..."
ensure_milestone "v0.2 Deterministic Core" "no_std deterministic core, command buffer, baseline telemetry and golden tests."
ensure_milestone "v0.3 Portable Backends + Partial Redraw" "Backend abstraction across devices, dirty rects, partial presents, buffering and profiles."
ensure_milestone "v0.4 Performance Architecture" "Tile/bin renderer, fixed-point options, quality tiers and profiling hooks."
ensure_milestone "v0.5 Asset Pipeline + Streaming" "Offline asset tooling, chunked scene format, streaming IO, and budget validation."
ensure_milestone "v0.6 Reliability + Release Candidate" "Fail-soft runtime behavior, regression infra, docs, and release hardening."

echo "Creating roadmap issues..."
create_issue \
  "Establish strict no_std crate boundaries for render core" \
  "v0.2 Deterministic Core" \
  "priority:p0,type:feature,area:architecture,platform:m3,platform:m4,platform:m33,platform:m55" \
  "## Goal
Create a strict \`no_std\` core API with no heap allocation in the frame path.

## Acceptance criteria
- [ ] Core builds with \`--no-default-features\`
- [ ] Frame loop has no allocator calls
- [ ] Architecture doc explains crate boundaries"

create_issue \
  "Add compile-time resource caps and typed over-budget errors" \
  "v0.2 Deterministic Core" \
  "priority:p0,type:feature,area:memory" \
  "## Goal
Guarantee bounded memory usage via compile-time limits.

## Acceptance criteria
- [ ] Max caps for vertices/indices/draws/lights/textures
- [ ] Exceeding caps returns typed errors
- [ ] Limits documented per target profile"

create_issue \
  "Implement command buffer record/execute render flow" \
  "v0.2 Deterministic Core" \
  "priority:p0,type:feature,area:renderer" \
  "## Goal
Decouple scene update from deterministic render execution.

## Acceptance criteria
- [ ] Command list records draw intent
- [ ] Deterministic replay order
- [ ] Host tests prove same output across repeated runs"

create_issue \
  "Ship golden-image regression tests for canonical scenes" \
  "v0.2 Deterministic Core" \
  "priority:p0,type:test,area:ci,area:renderer" \
  "## Goal
Catch rendering regressions reliably in CI.

## Acceptance criteria
- [ ] At least 3 canonical scenes
- [ ] Tolerance-based image diff
- [ ] CI gate enabled"

create_issue \
  "Define backend trait model for framebuffer and streaming present paths" \
  "v0.3 Portable Backends + Partial Redraw" \
  "priority:p0,type:feature,area:backend" \
  "## Goal
Create backend abstraction for different embedded display transports.

## Acceptance criteria
- [ ] Trait model supports framebuffer + streaming
- [ ] Core renderer depends only on backend traits
- [ ] Reference backend implementation compiles"

create_issue \
  "Implement dirty-rect tracking and partial present APIs" \
  "v0.3 Portable Backends + Partial Redraw" \
  "priority:p0,type:perf,area:renderer,area:backend,needs-benchmark" \
  "## Goal
Reduce display bandwidth and flush latency by updating changed regions only.

## Acceptance criteria
- [ ] Dirty-rect tracking in renderer
- [ ] Backend partial-present support
- [ ] Benchmark shows measurable bandwidth reduction"

create_issue \
  "Add double buffering with optional triple buffering feature gate" \
  "v0.3 Portable Backends + Partial Redraw" \
  "priority:p1,type:feature,area:renderer" \
  "## Goal
Improve frame pacing and reduce tearing/stalls.

## Acceptance criteria
- [ ] Double buffer mode is functional
- [ ] Optional triple buffering behind feature flag
- [ ] Memory impact documented"

create_issue \
  "Introduce target profile presets for M3/M4/M33/M55" \
  "v0.3 Portable Backends + Partial Redraw" \
  "priority:p1,type:feature,area:architecture,platform:m3,platform:m4,platform:m33,platform:m55" \
  "## Goal
Provide stable profile presets tuned for common Cortex-M classes.

## Acceptance criteria
- [ ] Profile presets added
- [ ] Each profile maps feature gates + limits
- [ ] Profile table documented in README/docs"

create_issue \
  "Implement tile/bin rendering mode with configurable tile sizes" \
  "v0.4 Performance Architecture" \
  "priority:p0,type:perf,area:raster,needs-benchmark" \
  "## Goal
Bound working set and improve cache behavior on constrained MCUs.

## Acceptance criteria
- [ ] Binning stage implemented
- [ ] Tile scheduler integrated
- [ ] Perf comparison vs non-tiled path captured"

create_issue \
  "Add fixed-point transform path for non-FPU or low-power targets" \
  "v0.4 Performance Architecture" \
  "priority:p0,type:perf,area:math,platform:m3,platform:m4,needs-benchmark" \
  "## Goal
Provide deterministic and efficient math path for MCU classes without strong FP throughput.

## Acceptance criteria
- [ ] Fixed-point transforms available behind feature flag
- [ ] Numeric error bounds documented
- [ ] Benchmark evidence captured"

create_issue \
  "Add fixed-point raster hotpath and compare against float pipeline" \
  "v0.4 Performance Architecture" \
  "priority:p1,type:perf,area:raster,area:math,needs-benchmark" \
  "## Goal
Improve raster throughput and predictability on constrained targets.

## Acceptance criteria
- [ ] Fixed-point raster hotpath implemented
- [ ] Visual parity tests pass
- [ ] Perf report included for representative scenes"

create_issue \
  "Implement renderer quality tiers and lightweight material profiles" \
  "v0.4 Performance Architecture" \
  "priority:p1,type:feature,area:renderer" \
  "## Goal
Expose explicit quality/performance tradeoffs by profile.

## Acceptance criteria
- [ ] \`fastest\`/ \`balanced\`/ \`quality\` tiers
- [ ] Material profiles for unlit and simple lambert
- [ ] Tier defaults mapped per target profile"

create_issue \
  "Integrate DWT cycle counter and optional RTT/ITM trace hooks" \
  "v0.4 Performance Architecture" \
  "priority:p0,type:perf,area:profiling,needs-hardware-validation" \
  "## Goal
Provide low-overhead, hardware-relevant performance instrumentation.

## Acceptance criteria
- [ ] DWT cycle timing API integrated
- [ ] Optional RTT/ITM sinks
- [ ] Profiling guide added"

create_issue \
  "Build offline asset converter CLI (mesh quantization + texture transcode)" \
  "v0.5 Asset Pipeline + Streaming" \
  "priority:p0,type:feature,area:assets" \
  "## Goal
Move expensive processing out of runtime and reduce on-device footprint.

## Acceptance criteria
- [ ] CLI converts source meshes to runtime-ready format
- [ ] Texture transcode path available
- [ ] Conversion is deterministic"

create_issue \
  "Define versioned chunked scene format optimized for flash/SD" \
  "v0.5 Asset Pipeline + Streaming" \
  "priority:p0,type:feature,area:assets,area:io" \
  "## Goal
Create a stable scene/container format for embedded streaming.

## Acceptance criteria
- [ ] Format spec documented
- [ ] Versioning policy defined
- [ ] Backward-compatibility strategy captured"

create_issue \
  "Implement cooperative chunk loader and non-blocking resource upload" \
  "v0.5 Asset Pipeline + Streaming" \
  "priority:p0,type:feature,area:io,area:renderer,needs-hardware-validation" \
  "## Goal
Enable streaming without introducing frame hitches.

## Acceptance criteria
- [ ] Cooperative/polled loading API
- [ ] Non-blocking upload path
- [ ] Hitch metrics validated on hardware"

create_issue \
  "Add build-time budget report and CI enforcement for profile limits" \
  "v0.5 Asset Pipeline + Streaming" \
  "priority:p1,type:test,area:ci,area:assets" \
  "## Goal
Prevent oversized assets/configurations from reaching device builds.

## Acceptance criteria
- [ ] RAM/flash/bandwidth report emitted
- [ ] Profile limits configurable
- [ ] CI fails on violations"

create_issue \
  "Add fail-soft degradation policy engine for over-budget frames" \
  "v0.6 Reliability + Release Candidate" \
  "priority:p0,type:feature,area:renderer" \
  "## Goal
Keep system responsive under transient overloads.

## Acceptance criteria
- [ ] Policy can drop/scale effects by priority
- [ ] Behavior deterministic and documented
- [ ] Telemetry reports degradation events"

create_issue \
  "Define typed runtime error taxonomy for renderer and backend faults" \
  "v0.6 Reliability + Release Candidate" \
  "priority:p0,type:refactor,area:architecture" \
  "## Goal
Improve diagnosability and integration reliability.

## Acceptance criteria
- [ ] Stable error enum hierarchy
- [ ] Distinct errors for budget/backend/stall paths
- [ ] Error handling guidelines documented"

create_issue \
  "Expand visual regression suite and per-profile performance baselines in CI" \
  "v0.6 Reliability + Release Candidate" \
  "priority:p0,type:test,area:ci,area:benchmark,needs-benchmark" \
  "## Goal
Catch visual and performance regressions before release.

## Acceptance criteria
- [ ] Golden scene matrix expanded
- [ ] Perf baselines per profile
- [ ] Regression thresholds enforced in CI"

create_issue \
  "Add hardware smoke-test workflow for representative boards" \
  "v0.6 Reliability + Release Candidate" \
  "priority:p1,type:test,area:ci,needs-hardware-validation,platform:m4,platform:m33,platform:m55" \
  "## Goal
Validate runtime behavior on real hardware before release.

## Acceptance criteria
- [ ] Smoke test harness/scripts added
- [ ] At least one representative board per class in matrix
- [ ] Failures produce actionable logs"

create_issue \
  "Publish backend integration checklist, memory sizing guide, and compatibility matrix" \
  "v0.6 Reliability + Release Candidate" \
  "priority:p1,type:docs,area:docs,platform:m3,platform:m4,platform:m33,platform:m55" \
  "## Goal
Enable repeatable board bring-up and deployment with clear constraints.

## Acceptance criteria
- [ ] Backend integration checklist
- [ ] Memory sizing/tuning cookbook
- [ ] Cortex-M compatibility matrix"

echo "Roadmap bootstrap complete."
echo "Next step: run this script and review created issues/milestones."
