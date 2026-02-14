#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "${script_dir}/common.sh"

init_rc=0
tau_demo_common_init "dashboard" "Seed deterministic dashboard state artifacts and run health/status inspection demo commands." "$@" || init_rc=$?
if [[ "${init_rc}" -eq 64 ]]; then
  exit 0
fi
if [[ "${init_rc}" -ne 0 ]]; then
  exit "${init_rc}"
fi

demo_state_dir=".tau/demo-dashboard"
demo_state_dir_abs="${TAU_DEMO_REPO_ROOT}/${demo_state_dir}"

tau_demo_common_prepare_binary

rm -rf "${demo_state_dir_abs}"
mkdir -p "${demo_state_dir_abs}"

cat > "${demo_state_dir_abs}/state.json" <<'JSON'
{
  "schema_version": 1,
  "processed_case_keys": ["snapshot:s1"],
  "widget_views": [{"widget_id":"health-summary"}],
  "control_audit": [{"case_id":"c1"}],
  "health": {
    "updated_unix_ms": 700,
    "cycle_duration_ms": 20,
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
{"reason_codes":["widget_views_updated"],"health_reason":"no recent transport failures observed"}
JSONL

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

tau_demo_common_finish
