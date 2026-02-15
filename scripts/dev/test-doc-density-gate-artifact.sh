#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT_UNDER_TEST="${SCRIPT_DIR}/doc-density-gate-artifact.sh"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for test-doc-density-gate-artifact.sh" >&2
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

output_json="${tmp_dir}/doc-density-gate-artifact.json"
output_md="${tmp_dir}/doc-density-gate-artifact.md"

bash -n "${SCRIPT_UNDER_TEST}"

"${SCRIPT_UNDER_TEST}" \
  --repo-root "${REPO_ROOT}" \
  --targets-file "docs/guides/doc-density-targets.json" \
  --output-json "${output_json}" \
  --output-md "${output_md}" \
  --generated-at "2026-02-15T13:00:00Z" \
  --quiet

if [[ ! -f "${output_json}" ]]; then
  echo "assertion failed (functional): expected JSON output artifact" >&2
  exit 1
fi
if [[ ! -f "${output_md}" ]]; then
  echo "assertion failed (functional): expected Markdown output artifact" >&2
  exit 1
fi

assert_equals "1" "$(jq -r '.schema_version' "${output_json}")" "conformance schema version"
assert_equals "2026-02-15T13:00:00Z" "$(jq -r '.generated_at' "${output_json}")" "conformance generated-at"

command_rendered="$(jq -r '.command.rendered' "${output_json}")"
assert_contains "${command_rendered}" "rust_doc_density.py" "functional command capture"
assert_contains "${command_rendered}" "--targets-file docs/guides/doc-density-targets.json" "functional command target capture"

python_version="$(jq -r '.versions.python3' "${output_json}")"
assert_contains "${python_version}" "Python" "functional python version capture"

git_commit="$(jq -r '.context.git_commit' "${output_json}")"
if [[ ! "${git_commit}" =~ ^[0-9a-f]{40}$ ]]; then
  echo "assertion failed (functional git commit): expected 40-char sha, got '${git_commit}'" >&2
  exit 1
fi

assert_contains "$(cat "${output_md}")" "## Troubleshooting" "conformance troubleshooting section"
assert_contains "$(cat "${output_md}")" "## Reproduction Command" "conformance reproduction section"

set +e
missing_targets_output="$(
  "${SCRIPT_UNDER_TEST}" \
    --repo-root "${REPO_ROOT}" \
    --targets-file "docs/guides/does-not-exist.json" \
    --output-json "${tmp_dir}/missing.json" \
    --output-md "${tmp_dir}/missing.md" 2>&1
)"
missing_targets_code=$?
set -e
assert_equals "1" "${missing_targets_code}" "regression missing targets exit"
assert_contains "${missing_targets_output}" "targets file not found" "regression missing targets message"

set +e
unknown_option_output="$("${SCRIPT_UNDER_TEST}" --unknown-flag 2>&1)"
unknown_option_code=$?
set -e
assert_equals "1" "${unknown_option_code}" "regression unknown option exit"
assert_contains "${unknown_option_output}" "unknown option '--unknown-flag'" "regression unknown option message"

echo "doc-density-gate-artifact tests passed"
