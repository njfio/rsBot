#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "${script_dir}/common.sh"

init_rc=0
tau_demo_common_init "browser-automation" "Run deterministic browser automation runtime and health-inspection demo commands against checked-in fixtures." "$@" || init_rc=$?
if [[ "${init_rc}" -eq 64 ]]; then
  exit 0
fi
if [[ "${init_rc}" -ne 0 ]]; then
  exit "${init_rc}"
fi

fixture_path="${TAU_DEMO_REPO_ROOT}/crates/tau-coding-agent/testdata/browser-automation-contract/mixed-outcomes.json"
demo_state_dir=".tau/demo-browser-automation"

tau_demo_common_require_file "${fixture_path}"
tau_demo_common_prepare_binary

rm -rf "${TAU_DEMO_REPO_ROOT}/${demo_state_dir}"

tau_demo_common_run_step \
  "browser-automation-runner" \
  --browser-automation-contract-runner \
  --browser-automation-fixture ./crates/tau-coding-agent/testdata/browser-automation-contract/mixed-outcomes.json \
  --browser-automation-state-dir "${demo_state_dir}" \
  --browser-automation-queue-limit 64 \
  --browser-automation-processed-case-cap 10000 \
  --browser-automation-retry-max-attempts 4 \
  --browser-automation-retry-base-delay-ms 0 \
  --browser-automation-action-timeout-ms 4000 \
  --browser-automation-max-actions-per-case 4

tau_demo_common_run_step \
  "transport-health-inspect-browser-automation" \
  --browser-automation-state-dir "${demo_state_dir}" \
  --transport-health-inspect browser-automation \
  --transport-health-json

tau_demo_common_run_step \
  "channel-store-inspect-browser-automation-fixtures" \
  --channel-store-root "${demo_state_dir}/channel-store" \
  --channel-store-inspect browser-automation/fixtures

tau_demo_common_finish
