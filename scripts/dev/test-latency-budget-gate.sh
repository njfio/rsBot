#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GATE_SCRIPT="${SCRIPT_DIR}/latency-budget-gate.sh"

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

policy_path="${tmp_dir}/policy.json"
report_pass_path="${tmp_dir}/report-pass.json"
report_fail_path="${tmp_dir}/report-fail.json"
report_invalid_path="${tmp_dir}/report-invalid.json"
output_pass_json="${tmp_dir}/gate-pass.json"
output_pass_md="${tmp_dir}/gate-pass.md"
output_fail_json="${tmp_dir}/gate-fail.json"
output_fail_md="${tmp_dir}/gate-fail.md"

cat >"${policy_path}" <<'EOF'
{
  "schema_version": 1,
  "max_fast_lane_median_ms": 1050,
  "min_improvement_percent": 0.5,
  "max_regression_percent": 0.0,
  "enforcement_mode": "fail",
  "remediation": {
    "fast_lane_median_ms": "Trim wrapper command scope or increase cache hit rate.",
    "improvement_percent": "Reduce command set breadth or optimize slowest wrapper."
  }
}
EOF

cat >"${report_pass_path}" <<'EOF'
{
  "schema_version": 1,
  "generated_at": "2026-02-16T14:20:00Z",
  "repository": "fixture/repo",
  "source_mode": "fixture",
  "baseline_report_path": "baseline.json",
  "baseline_median_ms": 1000,
  "fast_lane_median_ms": 900,
  "improvement_ms": 100,
  "improvement_percent": 10.0,
  "status": "improved",
  "wrappers": []
}
EOF

cat >"${report_fail_path}" <<'EOF'
{
  "schema_version": 1,
  "generated_at": "2026-02-16T14:20:00Z",
  "repository": "fixture/repo",
  "source_mode": "fixture",
  "baseline_report_path": "baseline.json",
  "baseline_median_ms": 1000,
  "fast_lane_median_ms": 1110,
  "improvement_ms": -110,
  "improvement_percent": -11.0,
  "status": "regressed",
  "wrappers": []
}
EOF

cat >"${report_invalid_path}" <<'EOF'
{
  "schema_version": 1,
  "generated_at": "2026-02-16T14:20:00Z",
  "repository": "fixture/repo"
}
EOF

"${GATE_SCRIPT}" \
  --quiet \
  --policy-json "${policy_path}" \
  --report-json "${report_pass_path}" \
  --generated-at "2026-02-16T15:00:00Z" \
  --output-json "${output_pass_json}" \
  --output-md "${output_pass_md}"

assert_equals "pass" "$(jq -r '.status' <"${output_pass_json}")" "functional pass status"
assert_equals "0" "$(jq -r '.violations | length' <"${output_pass_json}")" "functional pass violations"
assert_contains "$(cat "${output_pass_md}")" "| pass | 0 |" "functional pass markdown row"

set +e
fail_output="$("${GATE_SCRIPT}" --quiet --policy-json "${policy_path}" --report-json "${report_fail_path}" --output-json "${output_fail_json}" --output-md "${output_fail_md}" 2>&1)"
fail_exit=$?
set -e

if [[ ${fail_exit} -eq 0 ]]; then
  echo "assertion failed (functional fail path): expected non-zero exit" >&2
  exit 1
fi
assert_equals "fail" "$(jq -r '.status' <"${output_fail_json}")" "functional fail status"
assert_contains "${fail_output}" "budget violations" "functional fail stderr"
assert_contains "$(cat "${output_fail_md}")" "improvement_percent" "functional fail markdown diagnostics"

set +e
invalid_output="$("${GATE_SCRIPT}" --quiet --policy-json "${policy_path}" --report-json "${report_invalid_path}" 2>&1)"
invalid_exit=$?
set -e

if [[ ${invalid_exit} -eq 0 ]]; then
  echo "assertion failed (regression invalid report): expected non-zero exit" >&2
  exit 1
fi
assert_contains "${invalid_output}" "missing required report field" "regression invalid report error"

echo "latency-budget-gate tests passed"
