#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GUARD_SCRIPT="${SCRIPT_DIR}/panic-unsafe-guard.sh"

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

baseline_json="${tmp_dir}/baseline.json"
audit_pass_json="${tmp_dir}/audit-pass.json"
audit_fail_json="${tmp_dir}/audit-fail.json"

cat > "${baseline_json}" <<'JSON'
{
  "schema_version": 1,
  "thresholds": {
    "panic_total_max": 10,
    "panic_review_required_max": 0,
    "unsafe_total_max": 5,
    "unsafe_review_required_max": 0
  }
}
JSON

cat > "${audit_pass_json}" <<'JSON'
{
  "counters": {
    "panic_total": 10,
    "panic_review_required": 0,
    "unsafe_total": 5,
    "unsafe_review_required": 0
  }
}
JSON

cat > "${audit_fail_json}" <<'JSON'
{
  "counters": {
    "panic_total": 11,
    "panic_review_required": 1,
    "unsafe_total": 5,
    "unsafe_review_required": 0
  }
}
JSON

"${GUARD_SCRIPT}" --baseline-json "${baseline_json}" --audit-json "${audit_pass_json}" --quiet

set +e
fail_output="$(${GUARD_SCRIPT} --baseline-json "${baseline_json}" --audit-json "${audit_fail_json}" --quiet 2>&1)"
fail_code=$?
set -e

if [[ ${fail_code} -eq 0 ]]; then
  echo "assertion failed (guard regression): expected non-zero exit" >&2
  exit 1
fi

assert_contains "${fail_output}" "panic_total 11 > max 10" "panic total violation"
assert_contains "${fail_output}" "panic_review_required 1 > max 0" "panic review violation"

echo "panic-unsafe-guard tests passed"
