#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MATRIX_SCRIPT="${SCRIPT_DIR}/m21-validation-matrix.sh"

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for test-m21-validation-matrix.sh" >&2
  exit 1
fi

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

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

fixture_milestone="${tmp_dir}/milestone.json"
fixture_issues="${tmp_dir}/issues.json"
fixture_reports="${tmp_dir}/reports"
output_json="${tmp_dir}/matrix.json"
output_md="${tmp_dir}/matrix.md"

mkdir -p "${fixture_reports}"
cat >"${fixture_reports}/proof-a.json" <<'EOF'
{
  "schema_version": 1
}
EOF
cat >"${fixture_reports}/proof-b.md" <<'EOF'
# Proof B
EOF

cat >"${fixture_milestone}" <<'EOF'
{
  "number": 21,
  "title": "Gap Closure Wave 2026-03: Structural Runtime Hardening",
  "state": "open",
  "open_issues": 2,
  "closed_issues": 1,
  "due_on": "2026-03-30T00:00:00Z"
}
EOF

cat >"${fixture_issues}" <<'EOF'
[
  {
    "number": 1741,
    "title": "Subtask: Automated aggregation script for M21 validation matrix generation",
    "state": "open",
    "html_url": "https://github.com/njfio/Tau/issues/1741",
    "labels": [{"name": "task"}, {"name": "testing-matrix"}],
    "comments": 1
  },
  {
    "number": 1742,
    "title": "Subtask: Standard artifact manifest template for M21 live proof pack",
    "state": "closed",
    "html_url": "https://github.com/njfio/Tau/issues/1742",
    "labels": [{"name": "task"}, {"name": "testing-matrix"}],
    "comments": 2
  },
  {
    "number": 1759,
    "title": "Story: Cross-Wave Dependency Graph and Critical Path Control",
    "state": "open",
    "html_url": "https://github.com/njfio/Tau/issues/1759",
    "labels": [{"name": "story"}],
    "comments": 0
  },
  {
    "number": 9999,
    "title": "Synthetic PR record that should be excluded",
    "state": "closed",
    "html_url": "https://github.com/njfio/Tau/pull/9999",
    "labels": [{"name": "task"}],
    "comments": 0,
    "pull_request": {"url": "https://api.github.com/repos/njfio/Tau/pulls/9999"}
  }
]
EOF

bash -n "${MATRIX_SCRIPT}"

"${MATRIX_SCRIPT}" \
  --quiet \
  --repo "fixture/repository" \
  --milestone-number 21 \
  --fixture-milestone-json "${fixture_milestone}" \
  --fixture-issues-json "${fixture_issues}" \
  --reports-dir "${fixture_reports}" \
  --output-json "${output_json}" \
  --output-md "${output_md}" \
  --generated-at "2026-02-15T12:00:00Z"

if [[ ! -f "${output_json}" ]]; then
  echo "assertion failed (functional): missing output JSON" >&2
  exit 1
fi
if [[ ! -f "${output_md}" ]]; then
  echo "assertion failed (functional): missing output Markdown" >&2
  exit 1
fi

assert_equals "1" "$(jq -r '.schema_version' "${output_json}")" "functional schema version"
assert_equals "3" "$(jq -r '.summary.total_issues' "${output_json}")" "functional total issues"
assert_equals "1" "$(jq -r '.summary.closed_issues' "${output_json}")" "functional closed issues"
assert_equals "2" "$(jq -r '.summary.open_issues' "${output_json}")" "functional open issues"
assert_equals "2" "$(jq -r '.summary.testing_matrix_required' "${output_json}")" "functional testing required"
assert_equals "1" "$(jq -r '.summary.testing_matrix_closed' "${output_json}")" "functional testing closed"
assert_equals "2" "$(jq -r '.summary.local_artifacts_total' "${output_json}")" "functional local artifacts"
assert_equals "1741" "$(jq -r '.issues[0].number' "${output_json}")" "functional sorted issue order"
assert_equals "1759" "$(jq -r '.issues[2].number' "${output_json}")" "functional sorted issue order tail"
assert_contains "$(cat "${output_md}")" "## Issue Matrix" "functional markdown issue table"
assert_contains "$(cat "${output_md}")" "## Local Artifacts" "functional markdown artifact section"

set +e
missing_fixture_output="$(
  "${MATRIX_SCRIPT}" \
    --fixture-milestone-json "${fixture_milestone}" \
    --output-json "${tmp_dir}/missing.json" \
    --output-md "${tmp_dir}/missing.md" 2>&1
)"
missing_fixture_code=$?
set -e
assert_equals "1" "${missing_fixture_code}" "regression fixture pair requirement"
assert_contains "${missing_fixture_output}" "must be provided together" "regression fixture pair message"

set +e
invalid_number_output="$("${MATRIX_SCRIPT}" --milestone-number invalid 2>&1)"
invalid_number_code=$?
set -e
assert_equals "1" "${invalid_number_code}" "regression milestone number validation"
assert_contains "${invalid_number_output}" "must be a non-negative integer" "regression milestone number message"

set +e
unknown_option_output="$("${MATRIX_SCRIPT}" --unknown-flag 2>&1)"
unknown_option_code=$?
set -e
assert_equals "1" "${unknown_option_code}" "regression unknown option exit"
assert_contains "${unknown_option_output}" "unknown option '--unknown-flag'" "regression unknown option message"

echo "m21-validation-matrix tests passed"
