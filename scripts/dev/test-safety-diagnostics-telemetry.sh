#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

runtime_output_file="crates/tau-runtime/src/runtime_output_runtime.rs"
diagnostics_file="crates/tau-diagnostics/src/lib.rs"
quickstart_doc="docs/guides/quickstart.md"
operator_doc="docs/guides/operator-control-summary.md"

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "${haystack}" != *"${needle}"* ]]; then
    echo "assertion failed (${label}): expected to find '${needle}'" >&2
    exit 1
  fi
}

runtime_output_contents="$(cat "${runtime_output_file}")"
diagnostics_contents="$(cat "${diagnostics_file}")"
quickstart_contents="$(cat "${quickstart_doc}")"
operator_contents="$(cat "${operator_doc}")"

assert_contains "${runtime_output_contents}" "\"type\": \"safety_policy_applied\"" "runtime safety event type"
assert_contains "${runtime_output_contents}" "\"stage\": stage.as_str()" "runtime safety stage field"
assert_contains "${runtime_output_contents}" "\"blocked\": blocked" "runtime safety blocked field"
assert_contains "${runtime_output_contents}" "\"reason_codes\": reason_codes" "runtime safety reason codes field"
assert_contains "${runtime_output_contents}" "unit_event_to_json_maps_safety_policy_applied_shape" "runtime safety json mapping test"

assert_contains "${diagnostics_contents}" "PROMPT_TELEMETRY_RECORD_TYPE_V1" "diagnostics v1 record type constant"
assert_contains "${diagnostics_contents}" "PROMPT_TELEMETRY_SCHEMA_VERSION" "diagnostics schema version constant"
assert_contains "${diagnostics_contents}" "unit_summarize_audit_file_accepts_prompt_telemetry_v1_schema" "diagnostics unit schema test"
assert_contains "${diagnostics_contents}" "functional_summarize_audit_file_accepts_legacy_prompt_telemetry_fixture" "diagnostics functional legacy schema test"
assert_contains "${diagnostics_contents}" "regression_summarize_audit_file_ignores_future_prompt_telemetry_schema_versions" "diagnostics regression future schema test"

assert_contains "${operator_contents}" "## Safety Diagnostics Schema Contract" "operator schema contract heading"
assert_contains "${operator_contents}" "record_type=prompt_telemetry_v1" "operator v1 record type"
assert_contains "${operator_contents}" "schema_version=1" "operator schema version"

assert_contains "${quickstart_contents}" "## Safety Diagnostics and Telemetry Inspection" "quickstart safety telemetry heading"
assert_contains "${quickstart_contents}" "--json-events" "quickstart json events command"
assert_contains "${quickstart_contents}" "\"type\": \"safety_policy_applied\"" "quickstart sample output type"
assert_contains "${quickstart_contents}" "\"reason_codes\"" "quickstart sample output reason codes"

echo "safety-diagnostics-telemetry tests passed"
