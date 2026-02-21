#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
FIXTURE_ROOT="${REPO_ROOT}/scripts/dev/fixtures/panic-unsafe-audit"

output="$(bash "${REPO_ROOT}/scripts/dev/audit-panic-unsafe.sh" "${FIXTURE_ROOT}/crates")"

assert_contains() {
  local needle="$1"
  if ! grep -Fq "$needle" <<<"${output}"; then
    echo "missing expected output: ${needle}" >&2
    echo "--- output ---" >&2
    echo "${output}" >&2
    exit 1
  fi
}

assert_contains "panic_total=4"
assert_contains "panic_test_path=3"
assert_contains "panic_non_test_path=1"
assert_contains "unsafe_total=3"
assert_contains "unsafe_test_path=2"
assert_contains "unsafe_non_test_path=1"

echo "audit-panic-unsafe fixture conformance passed"
