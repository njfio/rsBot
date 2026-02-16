#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"

inventory_json="tasks/reports/m21-scaffold-inventory.json"
transports_doc="docs/guides/transports.md"

assert_equals() {
  local expected="$1"
  local actual="$2"
  local label="$3"
  if [[ "${expected}" != "${actual}" ]]; then
    echo "assertion failed (${label}): expected '${expected}' got '${actual}'"
    exit 1
  fi
}

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "${haystack}" != *"${needle}"* ]]; then
    echo "assertion failed (${label}): expected to find '${needle}'"
    exit 1
  fi
}

if [[ ! -f "${inventory_json}" ]]; then
  echo "missing required inventory artifact: ${inventory_json}"
  exit 1
fi

candidate_action="$(jq -r '.candidates[] | select(.candidate_id == "tau-contract-runner-remnants") | .action' "${inventory_json}")"
candidate_exists="$(jq -r '.candidates[] | select(.candidate_id == "tau-contract-runner-remnants") | .crate_exists' "${inventory_json}")"
runtime_hits="$(jq -r '.candidates[] | select(.candidate_id == "tau-contract-runner-remnants") | .runtime_reference_hits' "${inventory_json}")"
test_hits="$(jq -r '.candidates[] | select(.candidate_id == "tau-contract-runner-remnants") | .test_touchpoint_hits' "${inventory_json}")"

assert_equals "remove" "${candidate_action}" "functional inventory action"
assert_equals "false" "${candidate_exists}" "functional inventory crate exists"
assert_equals "0" "${runtime_hits}" "functional inventory runtime hits"
assert_equals "0" "${test_hits}" "functional inventory test hits"

# Regression: removed contract-runner flags must not appear in non-test startup-dispatch code.
for dispatch_file in crates/tau-coding-agent/src/startup_dispatch.rs crates/tau-onboarding/src/startup_dispatch.rs; do
  dispatch_non_test="$(awk '/^#\[cfg\(test\)\]/{exit} {print}' "${dispatch_file}")"
  for removed_field in memory_contract_runner dashboard_contract_runner browser_automation_contract_runner custom_command_contract_runner; do
    if [[ "${dispatch_non_test}" == *"${removed_field}"* ]]; then
      echo "assertion failed (regression dispatch remnant): found '${removed_field}' in non-test section of ${dispatch_file}"
      exit 1
    fi
  done
done

# Functional: demo scripts should not invoke removed contract-runner flags.
if rg -n -- '--(memory|dashboard|browser-automation|custom-command)-contract-runner' scripts/demo >/tmp/contract_runner_remnant_demo_hits.txt; then
  echo "assertion failed (functional demo remnants): removed contract-runner flags found in demo scripts"
  cat /tmp/contract_runner_remnant_demo_hits.txt
  exit 1
fi

transports_contents="$(cat "${transports_doc}")"
assert_contains "${transports_contents}" "## Removed Contract Runner Migration Matrix" "functional docs matrix heading"
assert_contains "${transports_contents}" '`--memory-contract-runner`' "functional docs memory removed flag"
assert_contains "${transports_contents}" '`--dashboard-contract-runner`' "functional docs dashboard removed flag"
assert_contains "${transports_contents}" '`--browser-automation-contract-runner`' "functional docs browser removed flag"
assert_contains "${transports_contents}" '`--custom-command-contract-runner`' "functional docs custom-command removed flag"

echo "contract-runner-remnants tests passed"
