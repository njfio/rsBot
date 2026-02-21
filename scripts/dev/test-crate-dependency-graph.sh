#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
SCRIPT_UNDER_TEST="${SCRIPT_DIR}/crate-dependency-graph.sh"
DOC_PATH="${REPO_ROOT}/docs/architecture/crate-dependency-diagram.md"

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
  echo "error: jq is required for test-crate-dependency-graph.sh" >&2
  exit 1
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

metadata_path="${tmp_dir}/metadata.json"
output_json="${tmp_dir}/crate-dependency-graph.json"
output_md="${tmp_dir}/crate-dependency-graph.md"

cat >"${metadata_path}" <<'JSON'
{
  "packages": [
    {
      "id": "path+file:///repo/crates/crate-a#0.1.0",
      "name": "crate-a",
      "manifest_path": "/repo/crates/crate-a/Cargo.toml",
      "dependencies": [{"name": "crate-b"}]
    },
    {
      "id": "path+file:///repo/crates/crate-b#0.1.0",
      "name": "crate-b",
      "manifest_path": "/repo/crates/crate-b/Cargo.toml",
      "dependencies": [{"name": "crate-c"}]
    },
    {
      "id": "path+file:///repo/crates/crate-c#0.1.0",
      "name": "crate-c",
      "manifest_path": "/repo/crates/crate-c/Cargo.toml",
      "dependencies": []
    },
    {
      "id": "registry+https://github.com/rust-lang/crates.io-index#serde@1.0.0",
      "name": "serde",
      "manifest_path": "/cargo/registry/serde/Cargo.toml",
      "dependencies": []
    }
  ],
  "workspace_members": [
    "path+file:///repo/crates/crate-a#0.1.0",
    "path+file:///repo/crates/crate-b#0.1.0",
    "path+file:///repo/crates/crate-c#0.1.0"
  ]
}
JSON

stdout_capture="$(${SCRIPT_UNDER_TEST} \
  --metadata "${metadata_path}" \
  --output-json "${output_json}" \
  --output-md "${output_md}" \
  --generated-at "2026-02-21T00:00:00Z")"

assert_contains "${stdout_capture}" "workspace_crates=3" "stdout crate count"
assert_equals "1" "$(jq -r '.schema_version' "${output_json}")" "json schema version"
assert_equals "2026-02-21T00:00:00Z" "$(jq -r '.generated_at' "${output_json}")" "json generated at"
assert_equals "3" "$(jq -r '.summary.workspace_crates' "${output_json}")" "json workspace crates"
assert_equals "2" "$(jq -r '.summary.workspace_edges' "${output_json}")" "json workspace edges"
assert_equals "crate-a,crate-b,crate-c" "$(jq -r '.crates | map(.name) | join(",")' "${output_json}")" "json crate names"
assert_equals "crate-a->crate-b,crate-b->crate-c" "$(jq -r '.edges | map("\(.from)->\(.to)") | join(",")' "${output_json}")" "json edges"
assert_contains "$(cat "${output_md}")" '```mermaid' "markdown mermaid block"
assert_contains "$(cat "${output_md}")" "crate_a --> crate_b" "markdown edge"

assert_contains "$(cat "${DOC_PATH}")" "scripts/dev/crate-dependency-graph.sh" "doc command contract"
assert_contains "$(cat "${DOC_PATH}")" "tasks/reports/crate-dependency-graph.json" "doc artifact reference"

echo "crate-dependency-graph tests passed"
