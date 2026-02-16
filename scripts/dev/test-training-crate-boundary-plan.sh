#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLAN_SCRIPT="${SCRIPT_DIR}/training-crate-boundary-plan.sh"

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

output_json="${tmp_dir}/plan.json"
output_md="${tmp_dir}/plan.md"

# Functional: default plan emits complete, non-ambiguous crate matrix.
"${PLAN_SCRIPT}" \
  --output-json "${output_json}" \
  --output-md "${output_md}" \
  --generated-at "2026-02-15T00:00:00Z" \
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
assert_equals "7" "$(jq -r '.summary.total_crates' "${output_json}")" "functional crate total"
assert_equals "7" "$(jq -r '.summary.retain_count' "${output_json}")" "functional retain count"
assert_equals "0" "$(jq -r '.summary.merge_count' "${output_json}")" "functional merge count"
assert_equals "0" "$(jq -r '.summary.ambiguous_count' "${output_json}")" "functional ambiguous count"
assert_equals "retain" "$(jq -r '.crates[] | select(.crate == "tau-training-store") | .decision' "${output_json}")" "functional store decision"
assert_equals "completed" "$(jq -r '.first_pr_sets[] | select(.id == "training-boundary-set-a") | .status' "${output_json}")" "functional first set status"
assert_equals "completed" "$(jq -r '.first_pr_sets[] | select(.id == "training-boundary-set-c") | .status' "${output_json}")" "functional set-c status"
assert_equals "#1628" "$(jq -r '.first_pr_sets[] | select(.id == "training-boundary-set-c") | .issues[0]' "${output_json}")" "functional set-c issue linkage"
assert_contains "$(cat "${output_md}")" "## Decision Matrix" "functional markdown section"

# Unit/Regression: invalid decision should fail closed.
invalid_decision_fixture="${tmp_dir}/invalid-decision.json"
cat >"${invalid_decision_fixture}" <<'EOF'
{
  "crates": [
    {
      "crate": "tau-training-types",
      "decision": "defer",
      "merge_target": null,
      "owner_surface": "types",
      "rationale": "bad decision token"
    },
    {
      "crate": "tau-training-store",
      "decision": "retain",
      "merge_target": null,
      "owner_surface": "store",
      "rationale": "ok"
    },
    {
      "crate": "tau-training-tracer",
      "decision": "retain",
      "merge_target": null,
      "owner_surface": "tracer",
      "rationale": "ok"
    },
    {
      "crate": "tau-training-runner",
      "decision": "retain",
      "merge_target": null,
      "owner_surface": "runner",
      "rationale": "ok"
    },
    {
      "crate": "tau-training-proxy",
      "decision": "retain",
      "merge_target": null,
      "owner_surface": "proxy",
      "rationale": "ok"
    },
    {
      "crate": "tau-trainer",
      "decision": "retain",
      "merge_target": null,
      "owner_surface": "trainer",
      "rationale": "ok"
    },
    {
      "crate": "tau-algorithm",
      "decision": "retain",
      "merge_target": null,
      "owner_surface": "algorithm",
      "rationale": "ok"
    }
  ],
  "first_pr_sets": [
    {
      "id": "set",
      "title": "set",
      "status": "planned",
      "issues": ["#1711"],
      "scope": ["x"],
      "test_matrix": ["unit"]
    }
  ]
}
EOF

set +e
invalid_output="$("${PLAN_SCRIPT}" \
  --fixture-plan-json "${invalid_decision_fixture}" \
  --output-json "${tmp_dir}/bad.json" \
  --output-md "${tmp_dir}/bad.md" \
  --quiet 2>&1)"
invalid_rc=$?
set -e

assert_equals "1" "${invalid_rc}" "unit invalid decision exit code"
assert_contains "${invalid_output}" "decision must be 'retain' or 'merge'" "unit invalid decision message"

# Regression: missing required crate should fail with explicit diagnostics.
missing_crate_fixture="${tmp_dir}/missing-crate.json"
cat >"${missing_crate_fixture}" <<'EOF'
{
  "crates": [
    {
      "crate": "tau-training-types",
      "decision": "retain",
      "merge_target": null,
      "owner_surface": "types",
      "rationale": "ok"
    },
    {
      "crate": "tau-training-store",
      "decision": "retain",
      "merge_target": null,
      "owner_surface": "store",
      "rationale": "ok"
    },
    {
      "crate": "tau-training-tracer",
      "decision": "retain",
      "merge_target": null,
      "owner_surface": "tracer",
      "rationale": "ok"
    },
    {
      "crate": "tau-training-runner",
      "decision": "retain",
      "merge_target": null,
      "owner_surface": "runner",
      "rationale": "ok"
    },
    {
      "crate": "tau-training-proxy",
      "decision": "retain",
      "merge_target": null,
      "owner_surface": "proxy",
      "rationale": "ok"
    },
    {
      "crate": "tau-trainer",
      "decision": "retain",
      "merge_target": null,
      "owner_surface": "trainer",
      "rationale": "ok"
    }
  ],
  "first_pr_sets": [
    {
      "id": "set",
      "title": "set",
      "status": "planned",
      "issues": ["#1711"],
      "scope": ["x"],
      "test_matrix": ["unit"]
    }
  ]
}
EOF

set +e
missing_output="$("${PLAN_SCRIPT}" \
  --fixture-plan-json "${missing_crate_fixture}" \
  --output-json "${tmp_dir}/missing.json" \
  --output-md "${tmp_dir}/missing.md" \
  --quiet 2>&1)"
missing_rc=$?
set -e

assert_equals "1" "${missing_rc}" "regression missing crate exit code"
assert_contains "${missing_output}" "missing required crates in plan: tau-algorithm" "regression missing crate message"

echo "training-crate-boundary-plan tests passed"
