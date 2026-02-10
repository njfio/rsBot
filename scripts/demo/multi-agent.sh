#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "${script_dir}/common.sh"

init_rc=0
tau_demo_common_init "multi-agent" "Run deterministic multi-agent runtime, health, and status-inspection demo commands against checked-in fixtures." "$@" || init_rc=$?
if [[ "${init_rc}" -eq 64 ]]; then
  exit 0
fi
if [[ "${init_rc}" -ne 0 ]]; then
  exit "${init_rc}"
fi

fixture_path="${TAU_DEMO_REPO_ROOT}/crates/tau-coding-agent/testdata/multi-agent-contract/rollout-pass.json"
demo_state_dir=".tau/demo-multi-agent"

tau_demo_common_require_file "${fixture_path}"
tau_demo_common_prepare_binary

rm -rf "${TAU_DEMO_REPO_ROOT}/${demo_state_dir}"

tau_demo_common_run_step \
  "multi-agent-runner" \
  --multi-agent-contract-runner \
  --multi-agent-fixture ./crates/tau-coding-agent/testdata/multi-agent-contract/rollout-pass.json \
  --multi-agent-state-dir "${demo_state_dir}" \
  --multi-agent-queue-limit 64 \
  --multi-agent-processed-case-cap 10000 \
  --multi-agent-retry-max-attempts 4 \
  --multi-agent-retry-base-delay-ms 0

tau_demo_common_run_step \
  "transport-health-inspect-multi-agent" \
  --multi-agent-state-dir "${demo_state_dir}" \
  --transport-health-inspect multi-agent \
  --transport-health-json

tau_demo_common_run_step \
  "multi-agent-status-inspect" \
  --multi-agent-state-dir "${demo_state_dir}" \
  --multi-agent-status-inspect \
  --multi-agent-status-json

tau_demo_common_run_step \
  "channel-store-inspect-multi-agent-router" \
  --channel-store-root "${demo_state_dir}/channel-store" \
  --channel-store-inspect multi-agent/orchestrator-router

tau_demo_common_finish
