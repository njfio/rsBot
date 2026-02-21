#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
CONTRIBUTING_PATH="${REPO_ROOT}/CONTRIBUTING.md"
SECURITY_PATH="${REPO_ROOT}/SECURITY.md"

assert_file_exists() {
  local path="$1"
  local label="$2"
  if [[ ! -f "${path}" ]]; then
    echo "assertion failed (${label}): missing file ${path}" >&2
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

assert_file_exists "${CONTRIBUTING_PATH}" "contributing exists"
assert_file_exists "${SECURITY_PATH}" "security exists"

contributing_contents="$(cat "${CONTRIBUTING_PATH}")"
security_contents="$(cat "${SECURITY_PATH}")"

assert_contains "${contributing_contents}" "## Development Workflow" "contributing workflow section"
assert_contains "${contributing_contents}" "## Testing and Quality Gates" "contributing test gates section"
assert_contains "${contributing_contents}" "## Pull Request Expectations" "contributing pr expectations section"

assert_contains "${security_contents}" "## Reporting a Vulnerability" "security reporting section"
assert_contains "${security_contents}" "## Response Expectations" "security response section"
assert_contains "${security_contents}" "## Coordinated Disclosure" "security disclosure section"

echo "contributor/security docs conformance passed"
