#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

safety_wrapper="scripts/demo/safety-smoke.sh"
index_script="scripts/demo/index.sh"
smoke_manifest=".github/demo-smoke-manifest.json"
ci_workflow=".github/workflows/ci.yml"
demo_guide="docs/guides/demo-index.md"

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "${haystack}" != *"${needle}"* ]]; then
    echo "assertion failed (${label}): expected to find '${needle}'" >&2
    exit 1
  fi
}

wrapper_contents="$(cat "${safety_wrapper}")"
index_contents="$(cat "${index_script}")"
manifest_contents="$(cat "${smoke_manifest}")"
workflow_contents="$(cat "${ci_workflow}")"
guide_contents="$(cat "${demo_guide}")"

assert_contains "${wrapper_contents}" "tau_demo_common_run_expect_failure" "wrapper fail-closed command helper"
assert_contains "${wrapper_contents}" "safety-prompt-injection-block" "wrapper scenario id"
assert_contains "${wrapper_contents}" "--prompt-sanitizer-mode block" "wrapper block mode"
assert_contains "${wrapper_contents}" "prompt_injection.ignore_instructions" "wrapper reason code marker"

assert_contains "${index_contents}" "\"safety-smoke\"" "index scenario registration"
assert_contains "${index_contents}" "[demo:safety-smoke] PASS safety-prompt-injection-block" "index expected marker"
assert_contains "${index_contents}" "Re-run ./scripts/demo/safety-smoke.sh --fail-fast" "index troubleshooting hint"

assert_contains "${manifest_contents}" "\"name\": \"safety-prompt-injection-block\"" "manifest safety command"
assert_contains "${manifest_contents}" "\"expected_exit_code\": 1" "manifest expected exit code"
assert_contains "${manifest_contents}" "\"stderr_contains\": \"prompt_injection.ignore_instructions\"" "manifest reason code check"

assert_contains "${workflow_contents}" "scripts/demo/test-safety-smoke.sh" "workflow safety smoke test step"
assert_contains "${workflow_contents}" ".github/demo-smoke-manifest.json" "workflow smoke manifest path"
assert_contains "${workflow_contents}" "--manifest .github/demo-smoke-manifest.json" "workflow manifest wiring"

assert_contains "${guide_contents}" "### safety-smoke" "guide safety section heading"
assert_contains "${guide_contents}" "./scripts/demo/safety-smoke.sh" "guide safety wrapper"
assert_contains "${guide_contents}" "[demo:safety-smoke] PASS safety-prompt-injection-block" "guide expected marker"
assert_contains "${guide_contents}" "prompt_injection.ignore_instructions" "guide troubleshooting reason code"

echo "safety-live-run-validation-contract tests passed"
