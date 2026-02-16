#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FAST_LANE_SCRIPT="${SCRIPT_DIR}/fast-lane-dev-loop.sh"

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "${haystack}" != *"${needle}"* ]]; then
    echo "assertion failed (${label}): expected output to contain '${needle}'" >&2
    echo "actual output:" >&2
    echo "${haystack}" >&2
    exit 1
  fi
}

assert_equals() {
  local expected="$1"
  local actual="$2"
  local label="$3"
  if [[ "${expected}" != "${actual}" ]]; then
    echo "assertion failed (${label}): expected '${expected}' got '${actual}'" >&2
    exit 1
  fi
}

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

baseline_fixture_path="${tmp_dir}/baseline.json"
benchmark_fixture_path="${tmp_dir}/benchmark.json"
output_json="${tmp_dir}/comparison.json"
output_md="${tmp_dir}/comparison.md"

cat >"${baseline_fixture_path}" <<'EOF'
{
  "schema_version": 1,
  "generated_at": "2026-02-16T13:10:00Z",
  "repository": "fixture/repo",
  "source_mode": "live",
  "environment": {
    "os": "linux",
    "arch": "x86_64",
    "shell": "bash",
    "rustc_version": "rustc 1.86.0",
    "cargo_version": "cargo 1.86.0"
  },
  "summary": {
    "command_count": 3,
    "run_count": 3,
    "failing_runs": 0,
    "slowest_command_id": "test-runtime-no-run"
  },
  "commands": [
    {
      "id": "check-tools",
      "command": "cargo check -p tau-tools --lib --target-dir target-fast",
      "run_count": 1,
      "stats": { "count": 1, "avg_ms": 1000, "p50_ms": 1000, "min_ms": 1000, "max_ms": 1000 },
      "nonzero_exit_count": 0,
      "invocation": "/bin/bash -lc \"cargo check -p tau-tools --lib --target-dir target-fast\""
    },
    {
      "id": "test-runtime-no-run",
      "command": "cargo test -p tau-github-issues-runtime --target-dir target-fast --no-run",
      "run_count": 1,
      "stats": { "count": 1, "avg_ms": 1200, "p50_ms": 1200, "min_ms": 1200, "max_ms": 1200 },
      "nonzero_exit_count": 0,
      "invocation": "/bin/bash -lc \"cargo test -p tau-github-issues-runtime --target-dir target-fast --no-run\""
    },
    {
      "id": "test-trainer-regression",
      "command": "cargo test -p tau-trainer --target-dir target-fast benchmark_artifact::tests::regression_summary_gate_report_manifest_ignores_non_json_files -- --nocapture",
      "run_count": 1,
      "stats": { "count": 1, "avg_ms": 900, "p50_ms": 900, "min_ms": 900, "max_ms": 900 },
      "nonzero_exit_count": 0,
      "invocation": "/bin/bash -lc \"cargo test -p tau-trainer --target-dir target-fast benchmark_artifact::tests::regression_summary_gate_report_manifest_ignores_non_json_files -- --nocapture\""
    }
  ],
  "hotspots": []
}
EOF

cat >"${benchmark_fixture_path}" <<'EOF'
{
  "wrappers": [
    {
      "id": "tools-check",
      "command": "cargo check -p tau-tools --lib --target-dir target-fast",
      "duration_ms": 840,
      "exit_code": 0,
      "use_case": "tools compile feedback"
    },
    {
      "id": "trainer-check",
      "command": "cargo check -p tau-trainer --lib --target-dir target-fast",
      "duration_ms": 860,
      "exit_code": 0,
      "use_case": "trainer compile feedback"
    },
    {
      "id": "trainer-smoke",
      "command": "cargo test -p tau-trainer --target-dir target-fast benchmark_artifact::tests::regression_summary_gate_report_manifest_ignores_non_json_files -- --nocapture",
      "duration_ms": 830,
      "exit_code": 0,
      "use_case": "trainer smoke regression"
    }
  ]
}
EOF

list_output="$("${FAST_LANE_SCRIPT}" list)"
assert_contains "${list_output}" "tools-check" "functional list tools wrapper"
assert_contains "${list_output}" "trainer-check" "functional list trainer-check wrapper"
assert_contains "${list_output}" "trainer-smoke" "functional list trainer wrapper"

"${FAST_LANE_SCRIPT}" benchmark \
  --fixture-json "${benchmark_fixture_path}" \
  --baseline-json "${baseline_fixture_path}" \
  --generated-at "2026-02-16T14:00:00Z" \
  --repo "fixture/repo" \
  --output-json "${output_json}" \
  --output-md "${output_md}" \
  --quiet

if [[ ! -f "${output_json}" ]]; then
  echo "assertion failed (functional json output): missing ${output_json}" >&2
  exit 1
fi
if [[ ! -f "${output_md}" ]]; then
  echo "assertion failed (functional markdown output): missing ${output_md}" >&2
  exit 1
fi

json_content="$(cat "${output_json}")"
md_content="$(cat "${output_md}")"

assert_equals "1" "$(jq -r '.schema_version' <<<"${json_content}")" "functional schema version"
assert_equals "1000" "$(jq -r '.baseline_median_ms' <<<"${json_content}")" "functional baseline median"
assert_equals "840" "$(jq -r '.fast_lane_median_ms' <<<"${json_content}")" "functional fast median"
assert_equals "160" "$(jq -r '.improvement_ms' <<<"${json_content}")" "functional improvement ms"
assert_equals "improved" "$(jq -r '.status' <<<"${json_content}")" "functional status"
assert_contains "${md_content}" "| improved | 1000 | 840 | 160 | 16.00 |" "functional markdown summary row"

set +e
invalid_output="$("${FAST_LANE_SCRIPT}" run unknown-wrapper 2>&1)"
invalid_exit=$?
set -e

if [[ ${invalid_exit} -eq 0 ]]; then
  echo "assertion failed (regression unknown wrapper): expected non-zero exit" >&2
  exit 1
fi
assert_contains "${invalid_output}" "unknown wrapper id" "regression unknown wrapper message"

echo "fast-lane-dev-loop tests passed"
