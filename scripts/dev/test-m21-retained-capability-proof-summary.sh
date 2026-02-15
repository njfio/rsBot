#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
SUMMARY_SCRIPT="${SCRIPT_DIR}/m21-retained-capability-proof-summary.sh"

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

mock_binary="${tmp_dir}/mock-tau-coding-agent.py"
cat >"${mock_binary}" <<'PY'
#!/usr/bin/env python3
import sys

print("mock-ok " + " ".join(sys.argv[1:]))
PY
chmod +x "${mock_binary}"

# Functional: default proof matrix runs cleanly with a mock binary and writes summary artifacts.
functional_reports_dir="${tmp_dir}/functional/reports"
functional_logs_dir="${tmp_dir}/functional/logs"
functional_json="${tmp_dir}/functional/summary.json"
functional_md="${tmp_dir}/functional/summary.md"

"${SUMMARY_SCRIPT}" \
  --repo-root "${REPO_ROOT}" \
  --binary "${mock_binary}" \
  --reports-dir "${functional_reports_dir}" \
  --logs-dir "${functional_logs_dir}" \
  --output-json "${functional_json}" \
  --output-md "${functional_md}" \
  --generated-at "2026-02-15T00:00:00Z" \
  --quiet

if [[ ! -f "${functional_json}" ]]; then
  echo "assertion failed (functional json output): missing ${functional_json}" >&2
  exit 1
fi
if [[ ! -f "${functional_md}" ]]; then
  echo "assertion failed (functional markdown output): missing ${functional_md}" >&2
  exit 1
fi

functional_json_content="$(cat "${functional_json}")"
functional_md_content="$(cat "${functional_md}")"

assert_equals "1" "$(jq -r '.schema_version' <<<"${functional_json_content}")" "functional schema version"
assert_equals "pass" "$(jq -r '.summary.status' <<<"${functional_json_content}")" "functional status"
assert_equals "3" "$(jq -r '.summary.total_runs' <<<"${functional_json_content}")" "functional total runs"
assert_equals "3" "$(jq -r '.summary.passed_runs' <<<"${functional_json_content}")" "functional passed runs"
assert_equals "0" "$(jq -r '.summary.failed_runs' <<<"${functional_json_content}")" "functional failed runs"
assert_equals "false" "$(jq -r '.report_paths.json | startswith("/")' <<<"${functional_json_content}")" "regression report json path portability"
assert_equals "false" "$(jq -r '.report_paths.markdown | startswith("/")' <<<"${functional_json_content}")" "regression report markdown path portability"
assert_equals "false" "$(jq -r '.report_paths.logs_dir | startswith("/")' <<<"${functional_json_content}")" "regression logs path portability"
assert_equals "false" "$(jq -r '.runs[0].stdout_log | startswith("/")' <<<"${functional_json_content}")" "regression run stdout path portability"
assert_equals "false" "$(jq -r '.runs[0].stderr_log | startswith("/")' <<<"${functional_json_content}")" "regression run stderr path portability"
assert_equals "true" "$(jq -r '.runs[] | select(.name == "demo-index-retained-scenarios") | .markers[] | select(.id == "index-summary-json") | .matched' <<<"${functional_json_content}")" "functional marker matched"
assert_equals "6" "$(find "${functional_logs_dir}" -type f | wc -l | tr -d '[:space:]')" "functional log file count"
assert_contains "${functional_md_content}" "## Run Matrix" "functional markdown matrix section"
assert_contains "${functional_md_content}" "| demo-index-retained-scenarios | pass |" "functional markdown run row"

# Regression: marker mismatch should fail with diagnosable report output.
regression_matrix="${tmp_dir}/regression-matrix.json"
cat >"${regression_matrix}" <<'EOF'
{
  "schema_version": 1,
  "name": "regression-matrix",
  "issues": ["#1746"],
  "runs": [
    {
      "name": "marker-regression",
      "description": "Intentional marker miss for regression validation.",
      "command": ["bash", "-lc", "echo retained-proof-regression"],
      "expected_exit_code": 0,
      "markers": [
        {
          "id": "missing-stdout-marker",
          "source": "stdout",
          "contains": "not-present-token"
        }
      ]
    }
  ]
}
EOF

regression_json="${tmp_dir}/regression/summary.json"
regression_md="${tmp_dir}/regression/summary.md"
regression_logs_dir="${tmp_dir}/regression/logs"
regression_reports_dir="${tmp_dir}/regression/reports"

set +e
"${SUMMARY_SCRIPT}" \
  --repo-root "${REPO_ROOT}" \
  --binary "${mock_binary}" \
  --matrix-json "${regression_matrix}" \
  --reports-dir "${regression_reports_dir}" \
  --logs-dir "${regression_logs_dir}" \
  --output-json "${regression_json}" \
  --output-md "${regression_md}" \
  --generated-at "2026-02-15T00:00:00Z" \
  --quiet
regression_rc=$?
set -e

assert_equals "1" "${regression_rc}" "regression exit code"
assert_equals "fail" "$(jq -r '.summary.status' "${regression_json}")" "regression summary status"
assert_equals "1" "$(jq -r '.summary.failed_runs' "${regression_json}")" "regression failed runs"
assert_equals "marker-missing:missing-stdout-marker" "$(jq -r '.runs[0].failure_reasons[0]' "${regression_json}")" "regression failure reason"
assert_contains "$(cat "${regression_md}")" "## Failure Diagnostics" "regression markdown failure section"
assert_contains "$(cat "${regression_md}")" "marker-missing:missing-stdout-marker" "regression markdown reason"

# Regression: default matrix should fail fast when binary is missing.
missing_binary_json="${tmp_dir}/missing-binary/summary.json"
missing_binary_md="${tmp_dir}/missing-binary/summary.md"
missing_binary_logs="${tmp_dir}/missing-binary/logs"
missing_binary_reports="${tmp_dir}/missing-binary/reports"

set +e
missing_binary_output="$("${SUMMARY_SCRIPT}" \
  --repo-root "${REPO_ROOT}" \
  --binary "${tmp_dir}/does-not-exist" \
  --reports-dir "${missing_binary_reports}" \
  --logs-dir "${missing_binary_logs}" \
  --output-json "${missing_binary_json}" \
  --output-md "${missing_binary_md}" \
  --generated-at "2026-02-15T00:00:00Z" \
  --quiet 2>&1)"
missing_binary_rc=$?
set -e

assert_equals "1" "${missing_binary_rc}" "regression missing binary exit code"
assert_contains "${missing_binary_output}" "binary path does not exist for run" "regression missing binary message"

echo "m21-retained-capability-proof-summary tests passed"
