#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "${script_dir}/common.sh"

init_rc=0
tau_demo_common_init "browser-automation-live" "Run deterministic browser automation live fixture execution through a mock Playwright CLI wrapper." "$@" || init_rc=$?
if [[ "${init_rc}" -eq 64 ]]; then
  exit 0
fi
if [[ "${init_rc}" -ne 0 ]]; then
  exit "${init_rc}"
fi

fixture_path="${TAU_DEMO_REPO_ROOT}/crates/tau-coding-agent/testdata/browser-automation-live/live-sequence.json"
playwright_cli_path="${TAU_DEMO_REPO_ROOT}/crates/tau-coding-agent/testdata/browser-automation-live/mock-playwright-cli.py"
demo_state_dir=".tau/demo-browser-automation-live"

tau_demo_common_require_file "${fixture_path}"
tau_demo_common_require_file "${playwright_cli_path}"
tau_demo_common_prepare_binary

rm -rf "${TAU_DEMO_REPO_ROOT}/${demo_state_dir}"

tau_demo_common_run_step \
  "browser-automation-live-runner" \
  --browser-automation-live-runner \
  --browser-automation-live-fixture ./crates/tau-coding-agent/testdata/browser-automation-live/live-sequence.json \
  --browser-automation-state-dir "${demo_state_dir}" \
  --browser-automation-playwright-cli ./crates/tau-coding-agent/testdata/browser-automation-live/mock-playwright-cli.py

tau_demo_common_run_step \
  "transport-health-inspect-browser-automation-live" \
  --browser-automation-state-dir "${demo_state_dir}" \
  --transport-health-inspect browser-automation \
  --transport-health-json

tau_demo_common_finish
