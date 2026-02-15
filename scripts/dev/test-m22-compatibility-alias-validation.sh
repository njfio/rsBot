#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT_UNDER_TEST="${SCRIPT_DIR}/m22-compatibility-alias-validation.sh"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for test-m22-compatibility-alias-validation.sh" >&2
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

fixture_results="${tmp_dir}/fixture-results.json"
output_json="${tmp_dir}/validation.json"
output_md="${tmp_dir}/validation.md"

cat >"${fixture_results}" <<'EOF'
{
  "commands": [
    {
      "name": "legacy_train_alias",
      "cmd": "cargo test -p tau-coding-agent legacy_train_aliases_with_warning_snapshot",
      "status": "pass",
      "stdout_excerpt": "ok"
    },
    {
      "name": "legacy_proxy_alias",
      "cmd": "cargo test -p tau-coding-agent legacy_training_aliases_with_warning_snapshot",
      "status": "pass",
      "stdout_excerpt": "ok"
    },
    {
      "name": "unknown_flag_fail_closed",
      "cmd": "cargo test -p tau-coding-agent prompt_optimization_alias_normalization_keeps_unknown_flags_fail_closed",
      "status": "pass",
      "stdout_excerpt": "ok"
    }
  ]
}
EOF

bash -n "${SCRIPT_UNDER_TEST}"

"${SCRIPT_UNDER_TEST}" \
  --repo-root "${REPO_ROOT}" \
  --fixture-results-json "${fixture_results}" \
  --output-json "${output_json}" \
  --output-md "${output_md}" \
  --generated-at "2026-02-15T18:00:00Z" \
  --quiet

if [[ ! -f "${output_json}" ]]; then
  echo "assertion failed (functional): output JSON missing" >&2
  exit 1
fi
if [[ ! -f "${output_md}" ]]; then
  echo "assertion failed (functional): output Markdown missing" >&2
  exit 1
fi

assert_equals "1" "$(jq -r '.schema_version' "${output_json}")" "conformance schema version"
assert_equals "2026-02-15T18:00:00Z" "$(jq -r '.generated_at' "${output_json}")" "conformance generated-at"
assert_equals "3" "$(jq -r '.summary.total' "${output_json}")" "functional total commands"
assert_equals "3" "$(jq -r '.summary.passed' "${output_json}")" "functional passed commands"
assert_equals "0" "$(jq -r '.summary.failed' "${output_json}")" "functional failed commands"

assert_contains "$(cat "${output_md}")" "## Command Results" "functional markdown section"
assert_contains "$(cat "${output_md}")" "legacy_train_alias" "functional markdown command row"
assert_contains "$(cat "${output_md}")" "Migration Policy" "functional markdown policy note"

set +e
missing_fixture_output="$(
  "${SCRIPT_UNDER_TEST}" \
    --repo-root "${REPO_ROOT}" \
    --fixture-results-json "${tmp_dir}/missing.json" \
    --output-json "${tmp_dir}/missing-out.json" \
    --output-md "${tmp_dir}/missing-out.md" 2>&1
)"
missing_fixture_code=$?
set -e
assert_equals "1" "${missing_fixture_code}" "regression missing fixture exit"
assert_contains "${missing_fixture_output}" "fixture results JSON not found" "regression missing fixture message"

set +e
unknown_option_output="$("${SCRIPT_UNDER_TEST}" --unknown-flag 2>&1)"
unknown_option_code=$?
set -e
assert_equals "1" "${unknown_option_code}" "regression unknown option exit"
assert_contains "${unknown_option_output}" "unknown option '--unknown-flag'" "regression unknown option message"

echo "m22-compatibility-alias-validation tests passed"
