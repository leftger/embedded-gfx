#!/usr/bin/env bash
# Runs unit tests, each integration test target, optional extra commands, and an integration check.
set -euo pipefail

cargo_test() {
  if [ -n "${CI_HOST_TARGET:-}" ]; then
    cargo test --target "$CI_HOST_TARGET" "$@"
  else
    cargo test "$@"
  fi
}

UNIT_ARGS="${CI_UNIT_TEST_ARGS:---lib --all-features}"
INTEGRATION_ARGS="${CI_INTEGRATION_TEST_ARGS:---all-features}"
DOC_ARGS="${CI_DOC_TEST_ARGS:---all-features}"

echo "::group::Unit tests"
# shellcheck disable=SC2086
cargo_test ${UNIT_ARGS}

if [ "${CI_RUN_DOCTESTS:-true}" = "true" ]; then
  echo "::endgroup::"
  echo "::group::Documentation tests"
  # shellcheck disable=SC2086
  cargo_test --doc ${DOC_ARGS}
fi

if [ -d tests ] && compgen -G "tests/*.rs" >/dev/null; then
  echo "::endgroup::"
  echo "::group::Integration tests"
  # Naming a test target whose `required-features` are unmet is a hard cargo
  # error, so read them from cargo metadata and enable them per target.
  required_features="$(python3 - <<'PY'
import json, subprocess

meta = json.loads(
    subprocess.check_output(["cargo", "metadata", "--no-deps", "--format-version", "1"])
)
for pkg in meta["packages"]:
    for target in pkg["targets"]:
        if "test" in target["kind"]:
            feats = target.get("required-features") or []
            print("{}\t{}".format(target["name"], ",".join(feats)))
PY
)"
  for test_file in tests/*.rs; do
    test_name="$(basename "$test_file" .rs)"
    feats="$(printf '%s\n' "$required_features" | awk -F'\t' -v n="$test_name" '$1 == n { print $2 }')"
    if [ -n "$feats" ]; then
      echo "Running integration test: ${test_name} (--features ${feats})"
      # shellcheck disable=SC2086
      cargo_test --test "${test_name}" --features "${feats}" ${INTEGRATION_ARGS}
    else
      echo "Running integration test: ${test_name}"
      # shellcheck disable=SC2086
      cargo_test --test "${test_name}" ${INTEGRATION_ARGS}
    fi
  done
fi

if [ -n "${CI_EXTRA_TEST_COMMANDS:-}" ]; then
  echo "::endgroup::"
  echo "::group::Feature tests"
  while IFS= read -r cmd; do
    [ -z "$cmd" ] && continue
    [[ "$cmd" == "|" ]] && continue
    echo "Running: ${cmd}"
    # shellcheck disable=SC2086
    eval "${cmd}"
  done <<< "${CI_EXTRA_TEST_COMMANDS}"
fi

echo "::endgroup::"
if [ "${CI_INTEGRATION_CHECK:-true}" = "true" ]; then
  echo "::group::Integration check"
  check_cmd="${CI_INTEGRATION_CHECK_CMD:-cargo build --examples --all-features}"
  echo "Running: ${check_cmd}"
  # shellcheck disable=SC2086
  eval "${check_cmd}"
  echo "::endgroup::"
fi

echo "All test and integration checks passed."
