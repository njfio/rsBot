#!/usr/bin/env bash
set -euo pipefail

# Verifies dashboard runtime consolidation on tau-gateway by executing mapped
# gateway dashboard endpoint/action/stream tests and onboarding regression.
#
# Usage:
#   scripts/dev/verify-dashboard-consolidation.sh
#
# Optional:
#   CARGO_TARGET_DIR=target-fast-dashboard scripts/dev/verify-dashboard-consolidation.sh

target_dir="${CARGO_TARGET_DIR:-target-fast-dashboard-consolidation}"

run_test() {
  local crate="$1"
  local test_name="$2"
  echo "==> cargo test -p ${crate} ${test_name}"
  CARGO_TARGET_DIR="${target_dir}" cargo test -p "${crate}" "${test_name}" -- --nocapture
}

run_cmd() {
  echo "==> $*"
  "$@"
}

# Conformance: dashboard runtime behavior is gateway-owned and exercised by
# endpoint + control + stream + auth regressions.
run_test "tau-gateway" "integration_dashboard_endpoints_return_state_health_widgets_timeline_and_alerts"
run_test "tau-gateway" "integration_dashboard_action_endpoint_writes_audit_and_updates_control_state"
run_test "tau-gateway" "integration_dashboard_stream_supports_reconnect_reset_and_snapshot_updates"
run_test "tau-gateway" "regression_dashboard_endpoints_reject_unauthorized_requests"

# Regression: onboarding transport validation rejects removed dashboard contract
# runner flag, enforcing consolidated runtime entrypoints.
run_test "tau-onboarding" "regression_validate_transport_mode_cli_rejects_removed_dashboard_contract_runner"

# Conformance: ADR for dashboard consolidation must exist.
run_cmd test -f docs/architecture/adr-001-dashboard-consolidation.md

echo "dashboard consolidation verification complete: all mapped checks passed."
