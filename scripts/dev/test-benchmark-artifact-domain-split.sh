#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

artifact_file="crates/tau-trainer/src/benchmark_artifact.rs"
tests_file="crates/tau-trainer/src/benchmark_artifact/tests.rs"

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "${haystack}" != *"${needle}"* ]]; then
    echo "assertion failed (${label}): expected to find '${needle}'" >&2
    exit 1
  fi
}

artifact_line_count="$(wc -l < "${artifact_file}" | tr -d ' ')"
if (( artifact_line_count >= 3000 )); then
  echo "assertion failed (line budget): expected ${artifact_file} < 3000 lines, got ${artifact_line_count}" >&2
  exit 1
fi

if [[ ! -f "${tests_file}" ]]; then
  echo "assertion failed (tests extraction file): missing ${tests_file}" >&2
  exit 1
fi

artifact_contents="$(cat "${artifact_file}")"
tests_contents="$(cat "${tests_file}")"

assert_contains "${artifact_contents}" "#[cfg(test)]" "artifact cfg-test marker"
assert_contains "${artifact_contents}" "mod tests;" "artifact external tests module marker"
assert_contains "${tests_contents}" "fn sample_policy_report()" "tests helper marker"
assert_contains "${tests_contents}" "fn spec_1980_c02_gate_report_summary_manifest_records_invalid_files_without_abort()" "tests regression marker"

echo "benchmark-artifact-domain-split tests passed"
