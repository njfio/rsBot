#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "${script_dir}/common.sh"

init_rc=0
tau_demo_common_init "gateway" "Run deterministic gateway runtime, health, and status-inspection demo commands against checked-in fixtures." "$@" || init_rc=$?
if [[ "${init_rc}" -eq 64 ]]; then
  exit 0
fi
if [[ "${init_rc}" -ne 0 ]]; then
  exit "${init_rc}"
fi

fixture_path="${TAU_DEMO_REPO_ROOT}/crates/tau-coding-agent/testdata/gateway-contract/rollout-pass.json"
demo_state_dir=".tau/demo-gateway"

tau_demo_common_require_file "${fixture_path}"
tau_demo_common_prepare_binary

rm -rf "${TAU_DEMO_REPO_ROOT}/${demo_state_dir}"

tau_demo_common_run_step \
  "gateway-service-start" \
  --gateway-state-dir "${demo_state_dir}" \
  --gateway-service-start

tau_demo_common_run_step \
  "gateway-runner" \
  --gateway-contract-runner \
  --gateway-fixture ./crates/tau-coding-agent/testdata/gateway-contract/rollout-pass.json \
  --gateway-state-dir "${demo_state_dir}"

tau_demo_common_run_step \
  "transport-health-inspect-gateway" \
  --gateway-state-dir "${demo_state_dir}" \
  --transport-health-inspect gateway \
  --transport-health-json

tau_demo_common_run_step \
  "gateway-status-inspect-running" \
  --gateway-state-dir "${demo_state_dir}" \
  --gateway-status-inspect \
  --gateway-status-json

tau_demo_common_run_step \
  "gateway-service-stop" \
  --gateway-state-dir "${demo_state_dir}" \
  --gateway-service-stop \
  --gateway-service-stop-reason demo_complete

tau_demo_common_run_step \
  "gateway-service-status-stopped" \
  --gateway-state-dir "${demo_state_dir}" \
  --gateway-service-status \
  --gateway-service-status-json

tau_demo_common_run_step \
  "gateway-status-inspect-stopped" \
  --gateway-state-dir "${demo_state_dir}" \
  --gateway-status-inspect \
  --gateway-status-json

tau_demo_common_run_step \
  "channel-store-inspect-gateway-ops-release-bot" \
  --channel-store-root "${demo_state_dir}/channel-store" \
  --channel-store-inspect gateway/ops-release-bot

tau_demo_common_finish
