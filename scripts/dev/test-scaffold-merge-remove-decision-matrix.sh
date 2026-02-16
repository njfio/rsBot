#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MATRIX_SCRIPT="${SCRIPT_DIR}/scaffold-merge-remove-decision-matrix.sh"

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

output_json="${tmp_dir}/matrix.json"
output_md="${tmp_dir}/matrix.md"
timestamp="2026-03-01T00:00:00Z"

# Functional + conformance: deterministic matrix generation and complete scoring coverage.
"${MATRIX_SCRIPT}" \
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
assert_equals "0" "$(jq -r '.summary.unresolved_count' "${output_json}")" "functional unresolved count"
assert_equals "scaffold-merge-remove-rubric" "$(jq -r '.policy_id' "${output_json}")" "functional policy id"
assert_equals "remove" "$(jq -r '.candidates[] | select(.candidate_id == "tau-contract-runner-remnants") | .action' "${output_json}")" "functional contract-runner action"
assert_contains "$(cat "${output_md}")" "## Decision Matrix" "functional markdown section"

json_hash_before="$(shasum -a 256 "${output_json}" | awk '{print $1}')"
md_hash_before="$(shasum -a 256 "${output_md}" | awk '{print $1}')"

"${MATRIX_SCRIPT}" \
  --output-json "${output_json}" \
  --output-md "${output_md}" \
  --generated-at "${timestamp}" \
  --quiet

json_hash_after="$(shasum -a 256 "${output_json}" | awk '{print $1}')"
md_hash_after="$(shasum -a 256 "${output_md}" | awk '{print $1}')"

assert_equals "${json_hash_before}" "${json_hash_after}" "conformance deterministic json hash"
assert_equals "${md_hash_before}" "${md_hash_after}" "conformance deterministic markdown hash"

# Regression: invalid metadata (blank owner) must fail closed.
bad_owner_fixture="${tmp_dir}/bad-owner.json"
cat >"${bad_owner_fixture}" <<'EOF'
{
  "candidates": [
    {
      "candidate_id": "fixture-candidate",
      "surface": "fixture surface",
      "owner": "   ",
      "operator_value": 3,
      "runtime_usage": 3,
      "maintenance_cost": 2,
      "test_posture": 3,
      "rationale": "fixture rationale"
    }
  ]
}
EOF

set +e
bad_owner_output="$("${MATRIX_SCRIPT}" \
  --fixture-candidates-json "${bad_owner_fixture}" \
  --output-json "${tmp_dir}/bad-owner-out.json" \
  --output-md "${tmp_dir}/bad-owner-out.md" \
  --quiet 2>&1)"
bad_owner_rc=$?
set -e

assert_equals "1" "${bad_owner_rc}" "regression bad owner exit"
assert_contains "${bad_owner_output}" "owner must be non-empty" "regression bad owner message"

# Regression: unresolved decisions must fail closed when policy disallows unresolved.
gap_policy_fixture="${tmp_dir}/gap-policy.json"
cat >"${gap_policy_fixture}" <<'EOF'
{
  "schema_version": 1,
  "policy_id": "fixture-gap-policy",
  "score_scale": { "min": 0, "max": 5 },
  "weights": {
    "operator_value": 3,
    "runtime_usage": 3,
    "maintenance_cost": 2,
    "test_posture": 2
  },
  "thresholds": {
    "remove_max_score": 10,
    "keep_min_score": 20,
    "keep_max_score": 22,
    "merge_min_score": 35
  },
  "unresolved_allowed": false
}
EOF

gap_candidate_fixture="${tmp_dir}/gap-candidate.json"
cat >"${gap_candidate_fixture}" <<'EOF'
{
  "candidates": [
    {
      "candidate_id": "gap-candidate",
      "surface": "gap surface",
      "owner": "runtime-core",
      "operator_value": 3,
      "runtime_usage": 2,
      "maintenance_cost": 3,
      "test_posture": 2,
      "rationale": "creates unresolved score gap"
    }
  ]
}
EOF

set +e
gap_output="$("${MATRIX_SCRIPT}" \
  --fixture-policy-json "${gap_policy_fixture}" \
  --fixture-candidates-json "${gap_candidate_fixture}" \
  --output-json "${tmp_dir}/gap-out.json" \
  --output-md "${tmp_dir}/gap-out.md" \
  --quiet 2>&1)"
gap_rc=$?
set -e

assert_equals "1" "${gap_rc}" "regression unresolved gap exit"
assert_contains "${gap_output}" "unresolved decisions are not allowed" "regression unresolved gap message"

echo "scaffold-merge-remove-decision-matrix tests passed"
