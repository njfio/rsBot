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

assert_contains "${test_contents}" "transport-sourced-prompt-injection.json" "inbound fixture corpus include"
assert_contains "${test_contents}" "functional_inbound_safety_fixture_corpus_applies_warn_and_redact_modes" "inbound functional coverage"
assert_contains "${test_contents}" "integration_inbound_safety_fixture_corpus_blocks_malicious_cases" "inbound integration coverage"
assert_contains "${test_contents}" "regression_inbound_safety_fixture_corpus_has_no_silent_pass_through_in_block_mode" "inbound regression coverage"
assert_contains "${test_contents}" "integration_tool_output_reinjection_fixture_suite_blocks_fail_closed" "tool output integration coverage"
assert_contains "${test_contents}" "regression_tool_output_reinjection_fixture_suite_emits_stable_stage_reason_codes" "tool output reason code regression"

assert_contains "${doc_contents}" "## Inbound and Tool-Output Safety Validation" "docs validation heading"
assert_contains "${doc_contents}" "functional_inbound_safety_fixture_corpus_applies_warn_and_redact_modes" "docs inbound command"
assert_contains "${doc_contents}" "integration_tool_output_reinjection_fixture_suite_blocks_fail_closed" "docs tool output command"
assert_contains "${doc_contents}" "regression_tool_output_reinjection_fixture_suite_emits_stable_stage_reason_codes" "docs reason code command"

echo "inbound-tool-output-safety-enforcement tests passed"
