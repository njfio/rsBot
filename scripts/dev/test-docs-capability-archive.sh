#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

README_PATH="${REPO_ROOT}/README.md"
CONTRIBUTING_PATH="${REPO_ROOT}/CONTRIBUTING.md"
SECURITY_PATH="${REPO_ROOT}/SECURITY.md"
OPERATOR_GUIDE_PATH="${REPO_ROOT}/docs/guides/operator-deployment-guide.md"
API_REFERENCE_PATH="${REPO_ROOT}/docs/guides/gateway-api-reference.md"
ARCHIVE_GUIDE_PATH="${REPO_ROOT}/docs/guides/spec-branch-archive-ops.md"

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

assert_not_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "${haystack}" == *"${needle}"* ]]; then
    echo "assertion failed (${label}): expected content to exclude '${needle}'" >&2
    exit 1
  fi
}

assert_file_exists "${README_PATH}" "readme exists"
assert_file_exists "${CONTRIBUTING_PATH}" "contributing exists"
assert_file_exists "${SECURITY_PATH}" "security policy exists"
assert_file_exists "${OPERATOR_GUIDE_PATH}" "operator guide exists"
assert_file_exists "${API_REFERENCE_PATH}" "api reference exists"
assert_file_exists "${ARCHIVE_GUIDE_PATH}" "archive guide exists"

readme_contents="$(cat "${README_PATH}")"
contributing_contents="$(cat "${CONTRIBUTING_PATH}")"
security_contents="$(cat "${SECURITY_PATH}")"
operator_contents="$(cat "${OPERATOR_GUIDE_PATH}")"
api_contents="$(cat "${API_REFERENCE_PATH}")"
archive_contents="$(cat "${ARCHIVE_GUIDE_PATH}")"

assert_contains "${readme_contents}" "## What Tau Is" "readme identity section"
assert_contains "${readme_contents}" "## Current Operator Surfaces" "readme operator surface section"
assert_contains "${readme_contents}" 'Operator deployment guide: `docs/guides/operator-deployment-guide.md`' "readme operator link"
assert_contains "${readme_contents}" 'Gateway API reference (70+ routes): `docs/guides/gateway-api-reference.md`' "readme api link"
assert_contains "${readme_contents}" 'Contributor guide: `CONTRIBUTING.md`' "readme contributing link"
assert_contains "${readme_contents}" 'Security policy: `SECURITY.md`' "readme security link"

assert_contains "${contributing_contents}" "## Prerequisites" "contributing prerequisites section"
assert_contains "${contributing_contents}" "## Issue and Spec Workflow" "contributing issue workflow section"
assert_contains "${contributing_contents}" "## Pull Request Checklist" "contributing pr checklist section"

assert_contains "${security_contents}" "## Supported Versions" "security supported versions section"
assert_contains "${security_contents}" "## Reporting Channels" "security reporting channels section"
assert_contains "${security_contents}" "## Triage and Response SLA" "security response sla section"

assert_contains "${operator_contents}" "## Quick Start (Localhost-Dev)" "operator quick start section"
assert_contains "${operator_contents}" "## Auth Modes and Tokens" "operator auth modes section"
assert_contains "${operator_contents}" "## Rollback Procedure" "operator rollback section"

assert_contains "${api_contents}" "**Route inventory:** 70 router route calls" "api route inventory marker"
assert_contains "${api_contents}" "## Endpoint Inventory (78 method-path entries)" "api method-path inventory marker"
assert_contains "${api_contents}" "## Route-Coverage Validation Procedure" "api validation section"
assert_contains "${api_contents}" "scripts/dev/gateway-api-route-inventory.sh" "api drift-check command"

assert_contains "${archive_contents}" "## Implemented Spec Archive" "archive guide implemented specs"
assert_contains "${archive_contents}" "## Merged Branch Archive Workflow" "archive guide branch workflow"
assert_contains "${archive_contents}" "scripts/dev/stale-merged-branch-prune.sh" "archive guide prune command"
assert_not_contains "${archive_contents}" "--dry-run" "archive guide stale dry-run flag"

echo "docs capability/archive conformance passed"
