#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
POLICY_SCRIPT="${SCRIPT_DIR}/oversized-file-policy.sh"

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

valid_json="${tmp_dir}/valid.json"
expired_json="${tmp_dir}/expired.json"
duplicate_json="${tmp_dir}/duplicate.json"
output_md="${tmp_dir}/policy.md"

cat >"${valid_json}" <<'EOF'
{
  "schema_version": 1,
  "exemptions": [
    {
      "path": "crates/tau-tools/src/tools.rs",
      "threshold_lines": 5600,
      "owner_issue": 1749,
      "rationale": "first-pass split landed; second pass in progress",
      "approved_by": "runtime-maintainer",
      "approved_at": "2026-02-10",
      "expires_on": "2026-03-10"
    }
  ]
}
EOF

cat >"${expired_json}" <<'EOF'
{
  "schema_version": 1,
  "exemptions": [
    {
      "path": "crates/tau-tools/src/tools.rs",
      "threshold_lines": 5600,
      "owner_issue": 1750,
      "rationale": "expired exemption fixture",
      "approved_by": "runtime-maintainer",
      "approved_at": "2026-01-01",
      "expires_on": "2026-01-15"
    }
  ]
}
EOF

cat >"${duplicate_json}" <<'EOF'
{
  "schema_version": 1,
  "exemptions": [
    {
      "path": "crates/tau-tools/src/tools.rs",
      "threshold_lines": 5600,
      "owner_issue": 1750,
      "rationale": "duplicate path A",
      "approved_by": "runtime-maintainer",
      "approved_at": "2026-02-10",
      "expires_on": "2026-03-10"
    },
    {
      "path": "crates/tau-tools/src/tools.rs",
      "threshold_lines": 5800,
      "owner_issue": 1751,
      "rationale": "duplicate path B",
      "approved_by": "runtime-maintainer",
      "approved_at": "2026-02-11",
      "expires_on": "2026-03-11"
    }
  ]
}
EOF

# Functional: valid metadata passes and emits markdown summary.
valid_output="$(
  "${POLICY_SCRIPT}" \
    --exemptions-json "${valid_json}" \
    --today 2026-02-15 \
    --output-md "${output_md}" \
    --quiet
)"
assert_equals "" "${valid_output}" "functional quiet mode output"
if [[ ! -f "${output_md}" ]]; then
  echo "assertion failed (functional markdown output): missing ${output_md}" >&2
  exit 1
fi
assert_contains "$(cat "${output_md}")" "docs/guides/oversized-file-policy.md" "functional markdown policy link"
assert_contains "$(cat "${output_md}")" "| crates/tau-tools/src/tools.rs | 5600 | 1749 | 2026-03-10 | runtime-maintainer |" "functional markdown row"

# Regression: expired exemptions fail validation with explicit reason.
set +e
expired_output="$(
  "${POLICY_SCRIPT}" \
    --exemptions-json "${expired_json}" \
    --today 2026-02-15 2>&1
)"
expired_exit=$?
set -e
if [[ ${expired_exit} -eq 0 ]]; then
  echo "assertion failed (regression expired exemption): expected non-zero exit" >&2
  exit 1
fi
assert_contains "${expired_output}" "is expired as of 2026-02-15" "regression expired message"

# Regression: duplicate paths are rejected for auditability.
set +e
duplicate_output="$(
  "${POLICY_SCRIPT}" \
    --exemptions-json "${duplicate_json}" \
    --today 2026-02-15 2>&1
)"
duplicate_exit=$?
set -e
if [[ ${duplicate_exit} -eq 0 ]]; then
  echo "assertion failed (regression duplicate path): expected non-zero exit" >&2
  exit 1
fi
assert_contains "${duplicate_output}" "duplicate exemption path" "regression duplicate message"

echo "oversized-file-policy tests passed"
