## Summary

- 

## Test Plan

- [ ] `cargo test --lib`
- [ ] `cargo test --test integration_tests`

## Telemetry / Baseline Change Checklist

Complete this only if telemetry thresholds or baseline snapshot expectations changed.

- [ ] I captured before/after telemetry snapshot numbers in this PR description.
- [ ] I described why the threshold change is required.
- [ ] I documented profile impact (`m3`, `m4`, `m33`, `m55`, `m55-perf-budget` as applicable).
- [ ] I confirmed fail-soft semantics still hold (`fallback_used` and `primary_budget_kind`).
- [ ] I updated relevant docs (`docs/perf-baselines.md`, `docs/compatibility-matrix.md`, `docs/caps-and-telemetry.md`) as needed.
