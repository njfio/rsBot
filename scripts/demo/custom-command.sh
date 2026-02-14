#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "${script_dir}/common.sh"

init_rc=0
tau_demo_common_init "custom-command" "Seed deterministic custom-command state artifacts and run health/status inspection demo commands." "$@" || init_rc=$?
if [[ "${init_rc}" -eq 64 ]]; then
  exit 0
fi
if [[ "${init_rc}" -ne 0 ]]; then
  exit "${init_rc}"
fi

demo_state_dir=".tau/demo-custom-command"
demo_state_dir_abs="${TAU_DEMO_REPO_ROOT}/${demo_state_dir}"

tau_demo_common_prepare_binary

rm -rf "${demo_state_dir_abs}"
mkdir -p "${demo_state_dir_abs}"

cat > "${demo_state_dir_abs}/state.json" <<'JSON'
{
  "schema_version": 1,
  "processed_case_keys": ["CREATE:deploy_release:create-1"],
  "commands": [
    {
      "case_key": "CREATE:deploy_release:create-1",
      "case_id": "create-1",
      "command_name": "deploy_release",
      "template": "deploy {{env}}",
      "operation": "CREATE",
      "last_status_code": 201,
      "last_outcome": "success",
      "run_count": 1,
      "updated_unix_ms": 1
    }
  ],
  "health": {
    "updated_unix_ms": 710,
    "cycle_duration_ms": 14,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 1,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
JSON

cat > "${demo_state_dir_abs}/runtime-events.jsonl" <<'JSONL'
{"reason_codes":["command_registry_mutated"],"health_reason":"no recent transport failures observed"}
JSONL

tau_demo_common_run_step \
  "transport-health-inspect-custom-command" \
  --custom-command-state-dir "${demo_state_dir}" \
  --transport-health-inspect custom-command \
  --transport-health-json

tau_demo_common_run_step \
  "custom-command-status-inspect" \
  --custom-command-state-dir "${demo_state_dir}" \
  --custom-command-status-inspect \
  --custom-command-status-json

tau_demo_common_finish
