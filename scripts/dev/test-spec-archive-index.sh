#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
SCRIPT_UNDER_TEST="${SCRIPT_DIR}/spec-archive-index.sh"

assert_equals() {
  local expected="$1"
  local actual="$2"
  local label="$3"
  if [[ "${expected}" != "${actual}" ]]; then
    echo "assertion failed (${label}): expected '${expected}', got '${actual}'" >&2
    exit 1
  fi
}

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "${haystack}" != *"${needle}"* ]]; then
    echo "assertion failed (${label}): expected content to include '${needle}'" >&2
    exit 1
  fi
}

if [[ ! -x "${SCRIPT_UNDER_TEST}" ]]; then
  echo "assertion failed (script exists): missing executable ${SCRIPT_UNDER_TEST}" >&2
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for test-spec-archive-index.sh" >&2
  exit 1
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

spec_root="${tmp_dir}/specs"
mkdir -p "${spec_root}/1001" "${spec_root}/1002" "${spec_root}/1003"

cat >"${spec_root}/1001/spec.md" <<'EOF'
# Spec 1001

Status: Implemented
EOF

cat >"${spec_root}/1002/spec.md" <<'EOF'
# Spec 1002

Status: Reviewed
EOF

cat >"${spec_root}/1003/spec.md" <<'EOF'
# Spec 1003

Status: Implemented
EOF

output_json="${tmp_dir}/spec-archive-index.json"
output_md="${tmp_dir}/spec-archive-index.md"

stdout_capture="$(
  "${SCRIPT_UNDER_TEST}" \
    --spec-root "${spec_root}" \
    --output-json "${output_json}" \
    --output-md "${output_md}" \
    --generated-at "2026-02-21T00:00:00Z"
)"

assert_contains "${stdout_capture}" "implemented_specs=2" "stdout implemented count"
assert_equals "1" "$(jq -r '.schema_version' "${output_json}")" "json schema version"
assert_equals "2026-02-21T00:00:00Z" "$(jq -r '.generated_at' "${output_json}")" "json generated at"
assert_equals "3" "$(jq -r '.summary.total_specs' "${output_json}")" "json total specs"
assert_equals "2" "$(jq -r '.summary.implemented_specs' "${output_json}")" "json implemented specs"
assert_equals "1001,1003" "$(jq -r '.implemented_specs | map(.issue_id) | join(",")' "${output_json}")" "json implemented ids"
assert_equals "specs/1001/spec.md" "$(jq -r '.specs[0].spec_path' "${output_json}")" "json relative spec path"
assert_contains "$(cat "${output_md}")" "# Implemented Spec Archive Index" "markdown title"
assert_contains "$(cat "${output_md}")" "| 1001 | Implemented |" "markdown implemented row"
assert_contains "$(cat "${output_md}")" "| 1002 | Reviewed |" "markdown reviewed row"

echo "spec-archive-index tests passed"
