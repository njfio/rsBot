#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "${script_dir}/common.sh"

init_rc=0
tau_demo_common_init "dashboard" "Run deterministic dashboard runtime, health, and status-inspection demo commands against checked-in fixtures." "$@" || init_rc=$?
if [[ "${init_rc}" -eq 64 ]]; then
  exit 0
fi
if [[ "${init_rc}" -ne 0 ]]; then
  exit "${init_rc}"
fi

fixture_path="${TAU_DEMO_REPO_ROOT}/crates/tau-coding-agent/testdata/dashboard-contract/snapshot-layout.json"
demo_state_dir=".tau/demo-dashboard"

tau_demo_common_require_file "${fixture_path}"
tau_demo_common_prepare_binary

rm -rf "${TAU_DEMO_REPO_ROOT}/${demo_state_dir}"

tau_demo_common_run_step \
  "dashboard-runner" \
  --dashboard-contract-runner \
  --dashboard-fixture ./crates/tau-coding-agent/testdata/dashboard-contract/snapshot-layout.json \
  --dashboard-state-dir "${demo_state_dir}" \
  --dashboard-queue-limit 64 \
  --dashboard-processed-case-cap 10000 \
  --dashboard-retry-max-attempts 4 \
  --dashboard-retry-base-delay-ms 0

tau_demo_common_run_step \
  "transport-health-inspect-dashboard" \
  --dashboard-state-dir "${demo_state_dir}" \
  --transport-health-inspect dashboard \
  --transport-health-json

tau_demo_common_run_step \
  "dashboard-status-inspect" \
  --dashboard-state-dir "${demo_state_dir}" \
  --dashboard-status-inspect \
  --dashboard-status-json

tau_demo_common_run_step \
  "channel-store-inspect-dashboard-operator" \
  --channel-store-root "${demo_state_dir}/channel-store" \
  --channel-store-inspect dashboard/operator:ops-release-1

tau_demo_common_finish
