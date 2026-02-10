#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "${script_dir}/common.sh"

init_rc=0
tau_demo_common_init "memory" "Run deterministic semantic memory runtime and health-inspection demo commands against checked-in fixtures." "$@" || init_rc=$?
if [[ "${init_rc}" -eq 64 ]]; then
  exit 0
fi
if [[ "${init_rc}" -ne 0 ]]; then
  exit "${init_rc}"
fi

fixture_path="${TAU_DEMO_REPO_ROOT}/crates/tau-coding-agent/testdata/memory-contract/retrieve-ranking.json"
demo_state_dir=".tau/demo-memory"

tau_demo_common_require_file "${fixture_path}"
tau_demo_common_prepare_binary

rm -rf "${TAU_DEMO_REPO_ROOT}/${demo_state_dir}"

tau_demo_common_run_step \
  "memory-runner" \
  --memory-contract-runner \
  --memory-fixture ./crates/tau-coding-agent/testdata/memory-contract/retrieve-ranking.json \
  --memory-state-dir "${demo_state_dir}" \
  --memory-queue-limit 64 \
  --memory-processed-case-cap 10000 \
  --memory-retry-max-attempts 4 \
  --memory-retry-base-delay-ms 0

tau_demo_common_run_step \
  "transport-health-inspect-memory" \
  --memory-state-dir "${demo_state_dir}" \
  --transport-health-inspect memory \
  --transport-health-json

tau_demo_common_run_step \
  "channel-store-inspect-memory-telegram" \
  --channel-store-root "${demo_state_dir}/channel-store" \
  --channel-store-inspect memory/telegram:ops

tau_demo_common_finish
