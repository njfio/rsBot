#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
MATRIX_JSON="${SCRIPT_DIR}/m21-retained-capability-proof-matrix.json"
VALIDATE_SCRIPT="${SCRIPT_DIR}/validate-m21-retained-capability-proof-matrix.sh"
SUMMARY_SCRIPT="${REPO_ROOT}/scripts/dev/m21-retained-capability-proof-summary.sh"

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

# Functional: matrix/checklist contract validates and enumerates retained capabilities.
validation_output="$("${VALIDATE_SCRIPT}" --repo-root "${REPO_ROOT}" --matrix-json "${MATRIX_JSON}")"
assert_contains "${validation_output}" "validation passed" "functional validation output"
assert_equals "5" "$(jq -r '.capabilities | length' "${MATRIX_JSON}")" "functional capability count"
assert_equals "3" "$(jq -r '.runs | length' "${MATRIX_JSON}")" "functional run count"
assert_equals "5" "$(jq -r '.artifact_checklist.required_artifacts | length' "${MATRIX_JSON}")" "functional artifact checklist count"
assert_equals "demo-index-retained-scenarios" "$(jq -r '.capabilities[] | select(.id == "onboarding") | .proof_step' "${MATRIX_JSON}")" "functional capability mapping"

# Regression: invalid matrix should fail with actionable diagnostics.
bad_matrix="${tmp_dir}/bad-matrix.json"
cat >"${bad_matrix}" <<'EOF'
{
  "schema_version": 1,
  "name": "bad-matrix",
  "artifact_checklist": {
    "required_fields": ["name", "path", "required", "status"],
    "required_artifacts": [
      { "name": "artifact-a", "path_token": "{reports_dir}/a.json", "producer": "scripts/demo/index.sh" }
    ]
  },
  "capabilities": [
    {
      "id": "broken-capability",
      "wrapper": "scripts/demo/does-not-exist.sh",
      "proof_step": "missing-run",
      "expected_markers": ["x"],
      "required_artifacts": ["artifact-a"]
    }
  ],
  "runs": [
    {
      "name": "run-a",
      "command": ["bash", "-lc", "echo ok"]
    }
  ]
}
EOF

set +e
bad_output="$("${VALIDATE_SCRIPT}" --repo-root "${REPO_ROOT}" --matrix-json "${bad_matrix}" 2>&1)"
bad_rc=$?
set -e
assert_equals "1" "${bad_rc}" "regression invalid matrix exit code"
assert_contains "${bad_output}" "wrapper does not exist" "regression invalid matrix message"

# Live-run proof: matrix is runnable by the retained-capability summary collector.
mock_binary="${tmp_dir}/mock-tau-coding-agent.py"
cat >"${mock_binary}" <<'PY'
#!/usr/bin/env python3
import sys
print("mock-ok " + " ".join(sys.argv[1:]))
PY
chmod +x "${mock_binary}"

live_reports_dir="${tmp_dir}/live/reports"
live_logs_dir="${tmp_dir}/live/logs"
live_json="${tmp_dir}/live/summary.json"
live_md="${tmp_dir}/live/summary.md"

"${SUMMARY_SCRIPT}" \
  --repo-root "${REPO_ROOT}" \
  --binary "${mock_binary}" \
  --matrix-json "${MATRIX_JSON}" \
  --reports-dir "${live_reports_dir}" \
  --logs-dir "${live_logs_dir}" \
  --output-json "${live_json}" \
  --output-md "${live_md}" \
  --generated-at "2026-02-15T00:00:00Z" \
  --quiet

assert_equals "pass" "$(jq -r '.summary.status' "${live_json}")" "live-run summary status"
assert_equals "3" "$(jq -r '.summary.total_runs' "${live_json}")" "live-run total runs"
assert_equals "3" "$(jq -r '.summary.passed_runs' "${live_json}")" "live-run passed runs"
assert_contains "$(cat "${live_md}")" "## Run Matrix" "live-run markdown matrix section"

echo "m21-retained-capability-proof-matrix tests passed"
