#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
DOC_PATH="${REPO_ROOT}/tasks/review-35.md"

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "${haystack}" != *"${needle}"* ]]; then
    echo "assertion failed (${label}): expected output to contain '${needle}'" >&2
    exit 1
  fi
}

assert_not_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "${haystack}" == *"${needle}"* ]]; then
    echo "assertion failed (${label}): expected output to NOT contain '${needle}'" >&2
    exit 1
  fi
}

if [[ ! -f "${DOC_PATH}" ]]; then
  echo "assertion failed (doc exists): missing ${DOC_PATH}" >&2
  exit 1
fi

doc_contents="$(cat "${DOC_PATH}")"

# Review #35 unresolved tracker must reflect delivered closures.
assert_contains "${doc_contents}" "| Cortex LLM wiring | Stubbed | Stubbed | Partial | **Done** |" "cortex llm resolved"
assert_contains "${doc_contents}" "| OpenTelemetry | Missing | Missing | Missing | **Done** |" "otel resolved"
assert_contains "${doc_contents}" "| Provider rate limiting | Missing | Missing | Missing | **Done** |" "provider rate limiting resolved"
assert_contains "${doc_contents}" "| Property-based testing | Minimal | Minimal | Minimal | **Improved** |" "property depth improved"

# Stale unresolved summary must be removed.
assert_not_contains "${doc_contents}" "Remaining: Cortex LLM (partial), property-based testing, OpenTelemetry, provider rate limiting." "remove stale remaining list"
assert_contains "${doc_contents}" "Remaining: property-based testing depth expansion beyond core rate-limit invariants." "updated remaining list"

echo "review-35 conformance passed"
