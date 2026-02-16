#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

guard_script=".github/scripts/oversized_file_guard.py"
guard_test=".github/scripts/test_oversized_file_guard.py"
policy_script_test="scripts/dev/test-oversized-file-policy.sh"
workflow_file=".github/workflows/ci.yml"
policy_doc="docs/guides/oversized-file-policy.md"
exemptions_json="tasks/policies/oversized-file-exemptions.json"

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "${haystack}" != *"${needle}"* ]]; then
    echo "assertion failed (${label}): expected to find '${needle}'" >&2
    exit 1
  fi
}

guard_contents="$(cat "${guard_script}")"
guard_test_contents="$(cat "${guard_test}")"
policy_test_contents="$(cat "${policy_script_test}")"
workflow_contents="$(cat "${workflow_file}")"
policy_doc_contents="$(cat "${policy_doc}")"
exemptions_contents="$(cat "${exemptions_json}")"

assert_contains "${guard_contents}" "--default-threshold" "guard default threshold arg"
assert_contains "${guard_contents}" "tasks/policies/oversized-file-exemptions.json" "guard exemptions file default"
assert_contains "${guard_contents}" "docs/guides/oversized-file-policy.md" "guard policy guide default"
assert_contains "${guard_contents}" "emit_annotations" "guard annotations helper"
assert_contains "${guard_contents}" "json-output-file" "guard json artifact output option"

assert_contains "${guard_test_contents}" "test_functional_cli_passes_with_exemption_and_writes_json" "guard functional unittest"
assert_contains "${guard_test_contents}" "test_regression_cli_emits_annotation_with_path_size_threshold_and_hint" "guard regression annotation unittest"
assert_contains "${guard_test_contents}" "test_regression_cli_reports_metadata_error_for_invalid_exemption_contract" "guard regression metadata unittest"

assert_contains "${policy_test_contents}" "oversized-file-policy tests passed" "policy bash test marker"

assert_contains "${workflow_contents}" "Check oversized production file thresholds" "workflow guard step"
assert_contains "${workflow_contents}" "python3 .github/scripts/oversized_file_guard.py" "workflow guard command"
assert_contains "${workflow_contents}" "Upload oversized-file guard artifact" "workflow artifact upload step"
assert_contains "${workflow_contents}" "ci-artifacts/oversized-file-guard.json" "workflow artifact path"

assert_contains "${exemptions_contents}" "\"schema_version\": 1" "exemptions schema version"
if ! jq -e '.schema_version == 1 and (.exemptions | type == "array")' "${exemptions_json}" >/dev/null; then
  echo "assertion failed (exemptions json shape): expected schema_version=1 with exemptions array" >&2
  exit 1
fi
if ! jq -e 'all(.exemptions[]?; (.owner_issue | type == "number") and (.expires_on | type == "string"))' "${exemptions_json}" >/dev/null; then
  echo "assertion failed (exemptions entry metadata): expected owner_issue/expires_on on each exemption entry" >&2
  exit 1
fi

assert_contains "${policy_doc_contents}" "## CI Guardrail Workflow Contract" "policy docs ci guardrail heading"
assert_contains "${policy_doc_contents}" "oversized_file_guard.py" "policy docs guard script reference"
assert_contains "${policy_doc_contents}" "ci-artifacts/oversized-file-guard.json" "policy docs artifact reference"
assert_contains "${policy_doc_contents}" "tasks/policies/oversized-file-exemptions.json" "policy docs exemptions reference"

echo "oversized-file-guardrail-contract tests passed"
