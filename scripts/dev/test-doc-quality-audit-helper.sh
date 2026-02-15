#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT_UNDER_TEST="${SCRIPT_DIR}/doc-quality-audit-helper.sh"

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for test-doc-quality-audit-helper.sh" >&2
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

fixture_root="${tmp_dir}/fixture"
mkdir -p "${fixture_root}/crates/alpha/src" "${fixture_root}/crates/beta/src"

cat >"${fixture_root}/crates/alpha/src/lib.rs" <<'EOF'
/// Assigns the value to the variable.
/// TODO: tighten this wording.
pub fn alpha() {}
EOF

cat >"${fixture_root}/crates/beta/src/lib.rs" <<'EOF'
/// TODO: acceptable for roadmap checklist.
pub fn beta() {}
EOF

policy_file="${tmp_dir}/policy.json"
cat >"${policy_file}" <<'EOF'
{
  "schema_version": 1,
  "patterns": [
    {
      "id": "assignment_narration",
      "description": "line-by-line narration pattern",
      "contains": "Assigns the value to the variable."
    },
    {
      "id": "todo_marker",
      "description": "TODO marker in rustdoc comment",
      "regex": "\\bTODO\\b"
    }
  ],
  "suppressions": [
    {
      "id": "beta_todo_allow",
      "path_contains": "crates/beta/src/lib.rs",
      "pattern_id": "todo_marker",
      "line_contains": "TODO: acceptable for roadmap checklist."
    }
  ]
}
EOF

output_json="${tmp_dir}/doc-quality-helper.json"
output_md="${tmp_dir}/doc-quality-helper.md"

bash -n "${SCRIPT_UNDER_TEST}"

stdout_capture="$(
  "${SCRIPT_UNDER_TEST}" \
    --repo-root "${fixture_root}" \
    --scan-root "crates" \
    --policy-file "${policy_file}" \
    --output-json "${output_json}" \
    --output-md "${output_md}" \
    --generated-at "2026-02-15T19:00:00Z"
)"

if [[ ! -f "${output_json}" ]]; then
  echo "assertion failed (functional): expected JSON output artifact" >&2
  exit 1
fi
if [[ ! -f "${output_md}" ]]; then
  echo "assertion failed (functional): expected Markdown output artifact" >&2
  exit 1
fi

assert_contains "${stdout_capture}" "findings=2" "functional findings summary"
assert_contains "${stdout_capture}" "suppressed=1" "functional suppression summary"

assert_equals "1" "$(jq -r '.schema_version' "${output_json}")" "conformance schema"
assert_equals "2026-02-15T19:00:00Z" "$(jq -r '.generated_at' "${output_json}")" "conformance generated-at"
assert_equals "2" "$(jq -r '.summary.findings_count' "${output_json}")" "functional findings count"
assert_equals "1" "$(jq -r '.summary.suppressed_count' "${output_json}")" "functional suppressed count"
assert_equals "assignment_narration" "$(jq -r '.findings[0].pattern_id' "${output_json}")" "functional first finding pattern"
assert_equals "todo_marker" "$(jq -r '.findings[1].pattern_id' "${output_json}")" "functional second finding pattern"

assert_contains "$(cat "${output_md}")" "# M23 Doc Quality Audit Helper Report" "functional markdown title"
assert_contains "$(cat "${output_md}")" "| assignment_narration |" "functional markdown finding row"
assert_contains "$(cat "${output_md}")" "| todo_marker |" "functional markdown finding row todo"

set +e
unknown_option_output="$("${SCRIPT_UNDER_TEST}" --unknown-option 2>&1)"
unknown_option_code=$?
set -e
assert_equals "1" "${unknown_option_code}" "regression unknown option exit"
assert_contains "${unknown_option_output}" "unknown option '--unknown-option'" "regression unknown option message"

set +e
missing_policy_output="$(
  "${SCRIPT_UNDER_TEST}" \
    --repo-root "${fixture_root}" \
    --scan-root "crates" \
    --policy-file "${tmp_dir}/missing.json" \
    --quiet 2>&1
)"
missing_policy_code=$?
set -e
assert_equals "1" "${missing_policy_code}" "regression missing policy exit"
assert_contains "${missing_policy_output}" "policy file not found" "regression missing policy message"

echo "doc-quality-audit-helper tests passed"
