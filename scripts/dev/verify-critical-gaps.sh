#!/usr/bin/env bash
set -euo pipefail

# Runs a focused validation sweep for previously reported critical gaps.
# Usage:
#   scripts/dev/verify-critical-gaps.sh
# Optional:
#   CARGO_TARGET_DIR=target-fast-critical scripts/dev/verify-critical-gaps.sh

target_dir="${CARGO_TARGET_DIR:-target-fast-critical-gaps}"

run_test() {
  local crate="$1"
  local test_name="$2"
  echo "==> cargo test -p ${crate} ${test_name}"
  CARGO_TARGET_DIR="${target_dir}" cargo test -p "${crate}" "${test_name}" -- --nocapture
}

run_test "tau-session" "integration_session_usage_summary_persists_across_store_reload"
run_test "tau-gateway" "integration_spec_c01_openresponses_preflight_blocks_over_budget_request"
run_test "tau-ai" "spec_c01_openai_serializes_prompt_cache_key_when_enabled"
run_test "tau-ai" "spec_c02_anthropic_serializes_system_cache_control_when_enabled"
run_test "tau-ai" "spec_c03_google_serializes_cached_content_reference_when_enabled"
run_test "tau-coding-agent" "spec_c02_integration_prompt_optimization_mode_executes_rl_optimizer_when_enabled"

echo "critical-gap verification complete: all mapped tests passed."
