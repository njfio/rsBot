#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

source_file="crates/tau-agent-core/src/lib.rs"
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

source_contents="$(cat "${source_file}")"
test_contents="$(cat "${test_file}")"
doc_contents="$(cat "${doc_file}")"

assert_contains "${source_contents}" "secret_leak.payload_serialization_failed" "runtime fail-closed reason code"
assert_contains "${source_contents}" "SafetyStage::OutboundHttpPayload.as_str().to_string()" "runtime outbound stage"

assert_contains "${test_contents}" "regression_prompt_safety_block_rejects_multiline_bypass_prompt" "regression inbound bypass"
assert_contains "${test_contents}" "regression_prompt_safety_block_prevents_multiline_tool_output_pass_through" "regression tool output bypass"
assert_contains "${test_contents}" "regression_secret_leak_block_rejects_project_scoped_openai_key_payload" "regression outbound fixture"
assert_contains "${test_contents}" "regression_secret_leak_block_fails_closed_when_outbound_payload_serialization_fails" "regression serialization fail-closed"

assert_contains "${doc_contents}" "## Safety Fail-Closed Semantics" "docs fail-closed heading"
assert_contains "${doc_contents}" "inbound_message" "docs inbound stage"
assert_contains "${doc_contents}" "tool_output" "docs tool output stage"
assert_contains "${doc_contents}" "outbound_http_payload" "docs outbound stage"

echo "safety-fail-closed-contract tests passed"
