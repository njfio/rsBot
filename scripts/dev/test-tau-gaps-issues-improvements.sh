#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
DOC_PATH="${REPO_ROOT}/tasks/tau-gaps-issues-improvements.md"

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

# Header/snapshot markers.
assert_contains "${doc_contents}" "# Tau: Gaps, Issues & Improvements (Review #31)" "header review marker"
assert_contains "${doc_contents}" "**Date:** 2026-02-21" "header date marker"
assert_contains "${doc_contents}" "**HEAD:** \`a3428b21\`" "header head marker"

# Roadmap closure rows refreshed from stale Open/Partial states.
assert_contains "${doc_contents}" "| 9 | Clean stale branches | Open | **Done** |" "row 9 closure"
assert_contains "${doc_contents}" "| 14 | Provider rate limiting | Partial | **Done** |" "row 14 closure"
assert_contains "${doc_contents}" "| 18 | OpenTelemetry | Open | **Done** |" "row 18 closure"
assert_contains "${doc_contents}" "| 21 | External coding agent (G21) | Open | **Done** |" "row 21 closure"

# Follow-up issue section must reflect closed state, not stale open wording.
assert_not_contains "${doc_contents}" "### 2.2 Open Follow-up Items" "remove stale follow-up heading"
assert_contains "${doc_contents}" "### 2.2 M104 Follow-up Issues (Current State)" "follow-up current-state heading"
assert_contains "${doc_contents}" "| OpenTelemetry export | #2616 | **Closed** |" "issue 2616 closed"
assert_contains "${doc_contents}" "| Provider-layer token-bucket rate limiting | #2611 | **Closed** |" "issue 2611 closed"
assert_contains "${doc_contents}" "| External coding-agent bridge protocol | #2619 | **Closed** |" "issue 2619 closed"

# Docs readiness rows should reflect delivered docs.
assert_not_contains "${doc_contents}" "| Operator deployment guide | **Missing** |" "operator guide no longer missing"
assert_contains "${doc_contents}" "| Operator deployment guide | **Done** | \`docs/guides/operator-deployment-guide.md\` |" "operator guide done"
assert_contains "${doc_contents}" "| API reference | **Done** | \`docs/guides/gateway-api-reference.md\` |" "api reference done"

echo "tau-gaps-issues-improvements conformance passed"
