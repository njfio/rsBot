#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SMOKE_SCRIPT="${SCRIPT_DIR}/tool-split-performance-smoke.sh"

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for test-tool-split-performance-smoke.sh" >&2
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

pass_fixture="${tmp_dir}/fixture-pass.json"
warn_fixture="${tmp_dir}/fixture-warn.json"
fail_fixture="${tmp_dir}/fixture-fail.json"
total_only_fixture="${tmp_dir}/fixture-total-only.json"

cat >"${pass_fixture}" <<'EOF'
{
  "samples": [
    { "name": "registry_dispatch", "elapsed_ms": 800 },
    { "name": "sessions_history", "elapsed_ms": 700 },
    { "name": "bash_dry_run", "elapsed_ms": 900 }
  ]
}
EOF

cat >"${warn_fixture}" <<'EOF'
{
  "samples": [
    { "name": "registry_dispatch", "elapsed_ms": 2000 },
    { "name": "sessions_history", "elapsed_ms": 1900 },
    { "name": "bash_dry_run", "elapsed_ms": 1600 }
  ]
}
EOF

cat >"${fail_fixture}" <<'EOF'
{
  "samples": [
    { "name": "registry_dispatch", "elapsed_ms": 3500 },
    { "name": "sessions_history", "elapsed_ms": 3100 },
    { "name": "bash_dry_run", "elapsed_ms": 2800 }
  ]
}
EOF

cat >"${total_only_fixture}" <<'EOF'
{
  "sample_total_ms": 2800
}
EOF

bash -n "${SMOKE_SCRIPT}"

pass_json="${tmp_dir}/pass.json"
pass_summary="${tmp_dir}/pass-summary.md"
"${SMOKE_SCRIPT}" \
  --quiet \
  --fixture-json "${pass_fixture}" \
  --baseline-ms 3000 \
  --warn-percent 75 \
  --fail-percent 200 \
  --output-json "${pass_json}" \
  --summary-file "${pass_summary}" >/dev/null

assert_equals "pass" "$(jq -r '.status' "${pass_json}")" "functional pass status"
assert_equals "2400" "$(jq -r '.sample_total_ms' "${pass_json}")" "functional pass total"
assert_equals "3" "$(jq -r '.samples | length' "${pass_json}")" "functional pass sample count"
assert_contains "$(cat "${pass_summary}")" "status: pass" "functional pass summary status"

total_only_json="${tmp_dir}/total-only.json"
"${SMOKE_SCRIPT}" \
  --quiet \
  --fixture-json "${total_only_fixture}" \
  --baseline-ms 3000 \
  --warn-percent 75 \
  --fail-percent 200 \
  --output-json "${total_only_json}" >/dev/null

assert_equals "pass" "$(jq -r '.status' "${total_only_json}")" "regression total-only status"
assert_equals "fixture_total" "$(jq -r '.samples[0].name' "${total_only_json}")" "regression total-only sample name"

warn_json="${tmp_dir}/warn.json"
warn_output="$(
  "${SMOKE_SCRIPT}" \
    --fixture-json "${warn_fixture}" \
    --baseline-ms 3000 \
    --warn-percent 75 \
    --fail-percent 200 \
    --output-json "${warn_json}" \
    --summary-file "${tmp_dir}/warn-summary.md" 2>&1
)"

assert_equals "warn" "$(jq -r '.status' "${warn_json}")" "regression warn status"
assert_contains "${warn_output}" "::warning::tool split performance smoke drift detected" "regression warn annotation"

fail_json="${tmp_dir}/fail.json"
set +e
fail_output="$(
  "${SMOKE_SCRIPT}" \
    --fixture-json "${fail_fixture}" \
    --baseline-ms 3000 \
    --warn-percent 75 \
    --fail-percent 200 \
    --output-json "${fail_json}" 2>&1
)"
fail_code=$?
set -e
assert_equals "1" "${fail_code}" "regression fail exit code"
assert_equals "fail" "$(jq -r '.status' "${fail_json}")" "regression fail status"
assert_contains "${fail_output}" "::error::tool split performance smoke threshold exceeded" "regression fail annotation"

set +e
unknown_output="$("${SMOKE_SCRIPT}" --unknown-flag 2>&1)"
unknown_code=$?
set -e
assert_equals "1" "${unknown_code}" "regression unknown option exit"
assert_contains "${unknown_output}" "error: unknown argument '--unknown-flag'" "regression unknown option message"

echo "tool-split-performance-smoke tests passed"
