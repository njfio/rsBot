#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
ROLLBACK_SCRIPT="${SCRIPT_DIR}/rollback-drill-checklist.sh"
SUMMARY_SCRIPT="${REPO_ROOT}/scripts/dev/m21-retained-capability-proof-summary.sh"
MATRIX_JSON="${SCRIPT_DIR}/m21-retained-capability-proof-matrix.json"

assert_equals() {
  local expected="$1"
  local actual="$2"
  local label="$3"
  if [[ "${expected}" != "${actual}" ]]; then
    echo "assertion failed (${label}): expected '${expected}' got '${actual}'" >&2
    exit 1
  fi
}

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

require_cmd() {
  local name="$1"
  if ! command -v "${name}" >/dev/null 2>&1; then
    echo "error: required command '${name}' not found" >&2
    exit 1
  fi
}

require_cmd jq
require_cmd python3

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

# Functional: pass-state artifacts produce rollback_required=false.
proof_pass="${tmp_dir}/proof-pass.json"
matrix_pass="${tmp_dir}/matrix-pass.json"
output_json="${tmp_dir}/rollback-pass.json"
output_md="${tmp_dir}/rollback-pass.md"

cat >"${proof_pass}" <<'EOF'
{
  "schema_version": 1,
  "summary": {
    "total_runs": 3,
    "passed_runs": 3,
    "failed_runs": 0,
    "status": "pass"
  },
  "report_paths": {
    "markdown": "tasks/reports/m21-retained-capability-proof-summary.md",
    "logs_dir": "tasks/reports/m21-retained-capability-proof-logs",
    "artifacts_dir": "tasks/reports/m21-retained-capability-artifacts"
  },
  "runs": [
    { "marker_summary": { "total": 3, "matched": 3, "missing": 0 } },
    { "marker_summary": { "total": 2, "matched": 2, "missing": 0 } }
  ]
}
EOF

cat >"${matrix_pass}" <<'EOF'
{
  "schema_version": 1,
  "summary": {
    "open_issues": 0,
    "completion_percent": 100.0
  }
}
EOF

"${ROLLBACK_SCRIPT}" \
  --repo-root "${REPO_ROOT}" \
  --proof-summary-json "${proof_pass}" \
  --validation-matrix-json "${matrix_pass}" \
  --output-json "${output_json}" \
  --output-md "${output_md}" \
  --generated-at "2026-02-15T00:00:00Z" \
  --quiet

assert_equals "false" "$(jq -r '.rollback_required' "${output_json}")" "functional rollback not required"
assert_equals "0" "$(jq -r '[.triggers[] | select(.active == true)] | length' "${output_json}")" "functional active triggers"
assert_contains "$(cat "${output_md}")" "Rollback required: no" "functional markdown status"

# Regression: failed proof artifacts should trigger rollback and fail with --fail-on-trigger.
proof_fail="${tmp_dir}/proof-fail.json"
cat >"${proof_fail}" <<'EOF'
{
  "schema_version": 1,
  "summary": {
    "total_runs": 3,
    "passed_runs": 2,
    "failed_runs": 1,
    "status": "fail"
  },
  "runs": [
    { "marker_summary": { "total": 3, "matched": 2, "missing": 1 } }
  ]
}
EOF

reg_json="${tmp_dir}/rollback-fail.json"
reg_md="${tmp_dir}/rollback-fail.md"

set +e
"${ROLLBACK_SCRIPT}" \
  --repo-root "${REPO_ROOT}" \
  --proof-summary-json "${proof_fail}" \
  --validation-matrix-json "${matrix_pass}" \
  --output-json "${reg_json}" \
  --output-md "${reg_md}" \
  --generated-at "2026-02-15T00:00:00Z" \
  --fail-on-trigger \
  --quiet
reg_rc=$?
set -e

assert_equals "1" "${reg_rc}" "regression fail-on-trigger exit code"
assert_equals "true" "$(jq -r '.rollback_required' "${reg_json}")" "regression rollback required"
assert_equals "true" "$(jq -r '.triggers[] | select(.id == "proof-runs-failed") | .active' "${reg_json}")" "regression failed-runs trigger"
assert_equals "true" "$(jq -r '.triggers[] | select(.id == "proof-markers-missing") | .active' "${reg_json}")" "regression missing-marker trigger"
assert_contains "$(cat "${reg_md}")" "Rollback required: yes" "regression markdown status"

# Live-run proof: generate real proof summary via matrix + mock binary, then build rollback checklist.
mock_binary="${tmp_dir}/mock-tau-coding-agent.py"
cat >"${mock_binary}" <<'PY'
#!/usr/bin/env python3
import sys
print("mock-ok " + " ".join(sys.argv[1:]))
PY
chmod +x "${mock_binary}"

live_reports_dir="${tmp_dir}/live/reports"
live_logs_dir="${tmp_dir}/live/logs"
live_proof_json="${tmp_dir}/live/proof-summary.json"
live_proof_md="${tmp_dir}/live/proof-summary.md"

"${SUMMARY_SCRIPT}" \
  --repo-root "${REPO_ROOT}" \
  --binary "${mock_binary}" \
  --matrix-json "${MATRIX_JSON}" \
  --reports-dir "${live_reports_dir}" \
  --logs-dir "${live_logs_dir}" \
  --output-json "${live_proof_json}" \
  --output-md "${live_proof_md}" \
  --generated-at "2026-02-15T00:00:00Z" \
  --quiet

live_matrix="${tmp_dir}/live/matrix.json"
cat >"${live_matrix}" <<'EOF'
{
  "schema_version": 1,
  "summary": {
    "open_issues": 0,
    "completion_percent": 100.0
  }
}
EOF

live_roll_json="${tmp_dir}/live/rollback.json"
live_roll_md="${tmp_dir}/live/rollback.md"

"${ROLLBACK_SCRIPT}" \
  --repo-root "${REPO_ROOT}" \
  --proof-summary-json "${live_proof_json}" \
  --validation-matrix-json "${live_matrix}" \
  --output-json "${live_roll_json}" \
  --output-md "${live_roll_md}" \
  --generated-at "2026-02-15T00:00:00Z" \
  --quiet

assert_equals "false" "$(jq -r '.rollback_required' "${live_roll_json}")" "live-run rollback not required"
assert_contains "$(cat "${live_roll_md}")" "## Rollback Drill Steps" "live-run markdown steps"

echo "rollback-drill-checklist tests passed"
