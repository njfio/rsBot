#!/usr/bin/env bash
set -euo pipefail

# Verifies roadmap claim closure for dynamic model-catalog discovery and remote
# merge policy using mapped tau-provider tests.
#
# Usage:
#   scripts/dev/verify-model-catalog-discovery-claim.sh
#
# Optional:
#   CARGO_TARGET_DIR=target-fast-model-catalog-discovery scripts/dev/verify-model-catalog-discovery-claim.sh

target_dir="${CARGO_TARGET_DIR:-target-fast-model-catalog-discovery}"

run_test() {
  local test_name="$1"
  echo "==> cargo test -p tau-provider ${test_name}"
  CARGO_TARGET_DIR="${target_dir}" cargo test -p tau-provider "${test_name}" -- --nocapture
}

run_test "spec_c01_parse_model_catalog_payload_accepts_openrouter_models_shape"
run_test "integration_model_catalog_remote_refresh_writes_cache_and_supports_offline_reuse"
run_test "integration_spec_c02_remote_refresh_merges_openrouter_entries_with_builtin_catalog"
run_test "regression_model_catalog_remote_failure_falls_back_to_cache"

rg -n "^- \\[x\\] Dynamic catalog discovery and remote-source merge policy\\." tasks/resolution-roadmap.md >/dev/null

echo "model catalog discovery claim verification complete: all mapped checks passed."
