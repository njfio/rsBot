#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INVENTORY_SCRIPT="${SCRIPT_DIR}/scaffold-inventory.sh"

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
require_cmd shasum

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

output_json="${tmp_dir}/inventory.json"
output_md="${tmp_dir}/inventory.md"
timestamp="2026-03-01T00:00:00Z"

# Functional + conformance: deterministic inventory generation with complete ownership map.
"${INVENTORY_SCRIPT}" \
  --output-json "${output_json}" \
  --output-md "${output_md}" \
  --generated-at "${timestamp}" \
  --quiet

if [[ ! -f "${output_json}" ]]; then
  echo "assertion failed (functional json output): missing ${output_json}" >&2
  exit 1
fi
if [[ ! -f "${output_md}" ]]; then
  echo "assertion failed (functional markdown output): missing ${output_md}" >&2
  exit 1
fi

assert_equals "1" "$(jq -r '.schema_version' "${output_json}")" "functional schema version"
assert_equals "${timestamp}" "$(jq -r '.generated_at' "${output_json}")" "functional generated_at"
assert_equals "13" "$(jq -r '.summary.total_candidates' "${output_json}")" "functional candidate total"
assert_equals "0" "$(jq -r '.summary.missing_owner_count' "${output_json}")" "functional missing owner count"
assert_equals "runtime-core" "$(jq -r '.candidates[] | select(.candidate_id == "tau-contract-runner-remnants") | .owner' "${output_json}")" "functional ownership mapping"
assert_contains "$(cat "${output_md}")" "## Update Instructions" "functional update instructions section"

json_hash_before="$(shasum -a 256 "${output_json}" | awk '{print $1}')"
md_hash_before="$(shasum -a 256 "${output_md}" | awk '{print $1}')"

"${INVENTORY_SCRIPT}" \
  --output-json "${output_json}" \
  --output-md "${output_md}" \
  --generated-at "${timestamp}" \
  --quiet

json_hash_after="$(shasum -a 256 "${output_json}" | awk '{print $1}')"
md_hash_after="$(shasum -a 256 "${output_md}" | awk '{print $1}')"

assert_equals "${json_hash_before}" "${json_hash_after}" "conformance deterministic json hash"
assert_equals "${md_hash_before}" "${md_hash_after}" "conformance deterministic markdown hash"

# Regression: blank owner metadata must fail closed.
bad_owner_fixture="${tmp_dir}/bad-owner.json"
cat >"${bad_owner_fixture}" <<'EOF'
{
  "candidates": [
    {
      "candidate_id": "fixture-candidate",
      "surface": "crate:tau-fixture",
      "owner": "   ",
      "action": "keep"
    }
  ]
}
EOF

set +e
bad_owner_output="$("${INVENTORY_SCRIPT}" \
  --fixture-candidates-json "${bad_owner_fixture}" \
  --output-json "${tmp_dir}/bad-owner-out.json" \
  --output-md "${tmp_dir}/bad-owner-out.md" \
  --generated-at "${timestamp}" \
  --quiet 2>&1)"
bad_owner_rc=$?
set -e

assert_equals "1" "${bad_owner_rc}" "regression bad owner exit"
assert_contains "${bad_owner_output}" "owner must be non-empty" "regression bad owner message"

echo "scaffold-inventory tests passed"
