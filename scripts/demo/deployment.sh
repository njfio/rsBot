#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "${script_dir}/common.sh"

init_rc=0
tau_demo_common_init "deployment" "Run deterministic deployment runtime, health, and status-inspection demo commands against checked-in fixtures." "$@" || init_rc=$?
if [[ "${init_rc}" -eq 64 ]]; then
  exit 0
fi
if [[ "${init_rc}" -ne 0 ]]; then
  exit "${init_rc}"
fi

fixture_path="${TAU_DEMO_REPO_ROOT}/crates/tau-coding-agent/testdata/deployment-contract/rollout-pass.json"
demo_state_dir=".tau/demo-deployment"

tau_demo_common_require_file "${fixture_path}"
tau_demo_common_prepare_binary

rm -rf "${TAU_DEMO_REPO_ROOT}/${demo_state_dir}"

tau_demo_common_run_step \
  "deployment-runner" \
  --deployment-contract-runner \
  --deployment-fixture ./crates/tau-coding-agent/testdata/deployment-contract/rollout-pass.json \
  --deployment-state-dir "${demo_state_dir}" \
  --deployment-queue-limit 64 \
  --deployment-processed-case-cap 10000 \
  --deployment-retry-max-attempts 4 \
  --deployment-retry-base-delay-ms 0

tau_demo_common_run_step \
  "transport-health-inspect-deployment" \
  --deployment-state-dir "${demo_state_dir}" \
  --transport-health-inspect deployment \
  --transport-health-json

tau_demo_common_run_step \
  "deployment-status-inspect" \
  --deployment-state-dir "${demo_state_dir}" \
  --deployment-status-inspect \
  --deployment-status-json

tau_demo_common_run_step \
  "channel-store-inspect-deployment-edge-wasm" \
  --channel-store-root "${demo_state_dir}/channel-store" \
  --channel-store-inspect deployment/edge-wasm

tau_demo_common_finish
