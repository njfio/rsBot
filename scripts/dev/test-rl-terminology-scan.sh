#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
SCRIPT_UNDER_TEST="${SCRIPT_DIR}/rl-terminology-scan.sh"
ALLOWLIST_REL="tasks/policies/rl-terms-allowlist.json"

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for test-rl-terminology-scan.sh" >&2
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

fixture_root="${tmp_dir}/fixture-repo"
mkdir -p "${fixture_root}/docs/guides" "${fixture_root}/docs/planning"

cat >"${fixture_root}/docs/planning/future-true-rl-roadmap.md" <<'EOF'
# Future RL Roadmap

This section tracks future true-RL milestones.
We plan reinforcement learning policy training in Q3.
EOF

cat >"${fixture_root}/docs/guides/training-ops.md" <<'EOF'
# Training Ops

Current RL mode is available for all operators.
EOF

cat >"${fixture_root}/docs/guides/misc.md" <<'EOF'
# Misc

We mention reinforcement learning here without future context.
EOF

output_json="${tmp_dir}/scan.json"
output_md="${tmp_dir}/scan.md"

bash -n "${SCRIPT_UNDER_TEST}"

"${SCRIPT_UNDER_TEST}" \
  --repo-root "${REPO_ROOT}" \
  --scan-root "${fixture_root}" \
  --allowlist-file "${ALLOWLIST_REL}" \
  --output-json "${output_json}" \
  --output-md "${output_md}" \
  --generated-at "2026-02-15T17:30:00Z" \
  --quiet

if [[ ! -f "${output_json}" ]]; then
  echo "assertion failed (functional): missing output JSON" >&2
  exit 1
fi
if [[ ! -f "${output_md}" ]]; then
  echo "assertion failed (functional): missing output Markdown" >&2
  exit 1
fi

assert_equals "1" "$(jq -r '.schema_version' "${output_json}")" "conformance schema version"
assert_equals "2026-02-15T17:30:00Z" "$(jq -r '.generated_at' "${output_json}")" "conformance generated-at"

assert_equals "1" "$(jq -r '.summary.approved_count' "${output_json}")" "functional approved count"
assert_equals "2" "$(jq -r '.summary.stale_count' "${output_json}")" "functional stale count"
assert_contains "$(jq -r '.approved_matches[0].path' "${output_json}")" "future-true-rl-roadmap.md" "conformance approved path"

stale_paths="$(jq -r '.stale_findings[].path' "${output_json}")"
assert_contains "${stale_paths}" "training-ops.md" "regression stale path (known stale)"
assert_contains "${stale_paths}" "misc.md" "regression stale path (missing context)"

set +e
missing_allowlist_output="$(
  "${SCRIPT_UNDER_TEST}" \
    --repo-root "${REPO_ROOT}" \
    --scan-root "${fixture_root}" \
    --allowlist-file "tasks/policies/does-not-exist.json" \
    --output-json "${tmp_dir}/missing.json" \
    --output-md "${tmp_dir}/missing.md" 2>&1
)"
missing_allowlist_code=$?
set -e
assert_equals "1" "${missing_allowlist_code}" "regression missing allowlist exit"
assert_contains "${missing_allowlist_output}" "allowlist file not found" "regression missing allowlist message"

set +e
unknown_option_output="$("${SCRIPT_UNDER_TEST}" --unknown-flag 2>&1)"
unknown_option_code=$?
set -e
assert_equals "1" "${unknown_option_code}" "regression unknown option exit"
assert_contains "${unknown_option_output}" "unknown option '--unknown-flag'" "regression unknown option message"

echo "rl-terminology-scan tests passed"
