# Hardware Smoke Test Workflow

This repository includes a hardware smoke workflow scaffold for representative boards.

## Workflow entrypoint

- `.github/workflows/hardware-smoke.yml`

The workflow is designed for `workflow_dispatch` and expects self-hosted runners with attached boards.

## Expected runner labels

- `self-hosted`
- `embedded-3dgfx-hw`
- board label (example: `stm32-m4`, `stm32-m33`, `cortex-m55`)

## Smoke script

- `.github/scripts/hardware_smoke.sh`

It emits standardized lines:

- `HARDWARE_SMOKE board=... profile=... stage=... status=...`

These are validated by:

- `.github/scripts/check_hardware_smoke.py`

## Minimum smoke stages

1. Build constrained profile for board.
2. Run deterministic telemetry snapshot tests.
3. Validate telemetry output format and expectations.
4. Persist smoke log artifact.

## Actionable failure output

Failures should include:

- board identifier
- profile
- failed stage
- command context

This allows triage without reproducing full local setup.
