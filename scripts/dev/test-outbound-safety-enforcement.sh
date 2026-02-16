#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

test_file="crates/tau-agent-core/src/tests/safety_pipeline.rs"
doc_file="docs/guides/quickstart.md"

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "${haystack}" != *"${needle}"* ]]; then
    echo "assertion failed (${label}): expected to find '${needle}'" >&2
    exit 1
  fi
}

test_contents="$(cat "${test_file}")"
doc_contents="$(cat "${doc_file}")"

assert_contains "${test_contents}" "outbound_secret_fixture_matrix.json" "outbound fixture include"
assert_contains "${test_contents}" "integration_secret_leak_policy_blocks_outbound_http_payload" "outbound block integration"
assert_contains "${test_contents}" "functional_secret_leak_policy_redacts_outbound_http_payload" "outbound redact functional"
assert_contains "${test_contents}" "integration_outbound_secret_fixture_matrix_blocks_all_cases" "outbound matrix block integration"
assert_contains "${test_contents}" "functional_outbound_secret_fixture_matrix_redacts_all_cases" "outbound matrix redact functional"
assert_contains "${test_contents}" "regression_outbound_secret_fixture_matrix_reason_codes_are_stable" "outbound reason code regression"
assert_contains "${test_contents}" "regression_secret_leak_block_fails_closed_when_outbound_payload_serialization_fails" "outbound fail-closed regression"

assert_contains "${doc_contents}" "## Outbound Payload Safety Validation" "docs outbound heading"
assert_contains "${doc_contents}" "integration_secret_leak_policy_blocks_outbound_http_payload" "docs outbound block command"
assert_contains "${doc_contents}" "functional_outbound_secret_fixture_matrix_redacts_all_cases" "docs outbound redact command"
assert_contains "${doc_contents}" "regression_outbound_secret_fixture_matrix_reason_codes_are_stable" "docs outbound reason code command"

echo "outbound-safety-enforcement tests passed"
