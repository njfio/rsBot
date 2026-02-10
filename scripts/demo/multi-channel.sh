#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "${script_dir}/common.sh"

init_rc=0
tau_demo_common_init "multi-channel" "Run deterministic multi-channel runtime and health-inspection demo commands against checked-in fixtures." "$@" || init_rc=$?
if [[ "${init_rc}" -eq 64 ]]; then
  exit 0
fi
if [[ "${init_rc}" -ne 0 ]]; then
  exit "${init_rc}"
fi

fixture_path="${TAU_DEMO_REPO_ROOT}/crates/tau-coding-agent/testdata/multi-channel-contract/baseline-three-channel.json"
demo_state_dir=".tau/demo-multi-channel"

tau_demo_common_require_file "${fixture_path}"
tau_demo_common_prepare_binary

rm -rf "${TAU_DEMO_REPO_ROOT}/${demo_state_dir}"

tau_demo_common_run_step \
  "multi-channel-runner" \
  --multi-channel-contract-runner \
  --multi-channel-fixture ./crates/tau-coding-agent/testdata/multi-channel-contract/baseline-three-channel.json \
  --multi-channel-state-dir "${demo_state_dir}" \
  --multi-channel-queue-limit 64 \
  --multi-channel-processed-event-cap 10000 \
  --multi-channel-retry-max-attempts 4 \
  --multi-channel-retry-base-delay-ms 0

tau_demo_common_run_step \
  "transport-health-inspect-multi-channel" \
  --multi-channel-state-dir "${demo_state_dir}" \
  --transport-health-inspect multi-channel \
  --transport-health-json

tau_demo_common_run_step \
  "channel-store-inspect-telegram" \
  --channel-store-root "${demo_state_dir}/channel-store" \
  --channel-store-inspect telegram/telegram-chat-42

tau_demo_common_finish
