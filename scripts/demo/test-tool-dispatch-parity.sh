#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PARITY_SCRIPT="${SCRIPT_DIR}/tool-dispatch-parity.sh"

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for test-tool-dispatch-parity.sh" >&2
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

pass_fixture="${tmp_dir}/parity-pass.json"
fail_fixture="${tmp_dir}/parity-fail.json"

cat >"${pass_fixture}" <<'EOF'
{
  "entries": [
    {
      "behavior": "Registry includes split session tools",
      "command": "cargo test -p tau-tools tools::tests::unit_builtin_agent_tool_name_registry_includes_session_tools -- --exact",
      "pass_criteria": "Command exits 0 and assertions pass.",
      "status": "pass",
      "elapsed_ms": 120
    },
    {
      "behavior": "Session history bounded lineage",
      "command": "cargo test -p tau-tools tools::tests::integration_sessions_history_tool_returns_bounded_lineage -- --exact",
      "pass_criteria": "Command exits 0 and assertions pass.",
      "status": "pass",
      "elapsed_ms": 140
    }
  ]
}
EOF

cat >"${fail_fixture}" <<'EOF'
{
  "entries": [
    {
      "behavior": "Registry includes split session tools",
      "command": "cargo test -p tau-tools tools::tests::unit_builtin_agent_tool_name_registry_includes_session_tools -- --exact",
      "pass_criteria": "Command exits 0 and assertions pass.",
      "status": "pass",
      "elapsed_ms": 120
    },
    {
      "behavior": "Session history bounded lineage",
      "command": "cargo test -p tau-tools tools::tests::integration_sessions_history_tool_returns_bounded_lineage -- --exact",
      "pass_criteria": "Command exits 0 and assertions pass.",
      "status": "fail",
      "elapsed_ms": 140,
      "exit_code": 1
    }
  ]
}
EOF

bash -n "${PARITY_SCRIPT}"

pass_md="${tmp_dir}/parity-pass.md"
pass_json="${tmp_dir}/parity-pass-out.json"
pass_summary="${tmp_dir}/parity-pass-summary.md"
"${PARITY_SCRIPT}" \
  --quiet \
  --fixture-json "${pass_fixture}" \
  --output-md "${pass_md}" \
  --output-json "${pass_json}" \
  --summary-file "${pass_summary}" >/dev/null

assert_equals "2" "$(jq -r '.passed' "${pass_json}")" "functional passed count"
assert_equals "0" "$(jq -r '.failed' "${pass_json}")" "functional failed count"
assert_equals "pass" "$(jq -r '.entries[0].status' "${pass_json}")" "functional first status"
assert_contains "$(cat "${pass_md}")" "Tool Dispatch Before/After Parity Checklist" "functional markdown header"
assert_contains "$(cat "${pass_summary}")" "failed: 0" "functional summary output"

fail_md="${tmp_dir}/parity-fail.md"
fail_json="${tmp_dir}/parity-fail-out.json"
set +e
fail_output="$(
  "${PARITY_SCRIPT}" \
    --fixture-json "${fail_fixture}" \
    --output-md "${fail_md}" \
    --output-json "${fail_json}" 2>&1
)"
fail_code=$?
set -e
assert_equals "1" "${fail_code}" "regression fail exit code"
assert_equals "1" "$(jq -r '.failed' "${fail_json}")" "regression failed count"
assert_contains "${fail_output}" "::error::tool dispatch parity checklist detected 1 failed behavior checks" "regression fail annotation"

set +e
unknown_output="$("${PARITY_SCRIPT}" --unknown-flag 2>&1)"
unknown_code=$?
set -e
assert_equals "1" "${unknown_code}" "regression unknown option exit"
assert_contains "${unknown_output}" "error: unknown argument '--unknown-flag'" "regression unknown option message"

echo "tool-dispatch-parity tests passed"
