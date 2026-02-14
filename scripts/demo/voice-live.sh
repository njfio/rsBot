#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "${script_dir}/common.sh"

init_rc=0
tau_demo_common_init "voice-live" "Run deterministic live voice runtime commands against checked-in fixtures and emit an artifact manifest." "$@" || init_rc=$?
if [[ "${init_rc}" -eq 64 ]]; then
  exit 0
fi
if [[ "${init_rc}" -ne 0 ]]; then
  exit "${init_rc}"
fi

single_turn_fixture="${TAU_DEMO_REPO_ROOT}/crates/tau-coding-agent/testdata/voice-live/single-turn.json"
multi_turn_fixture="${TAU_DEMO_REPO_ROOT}/crates/tau-voice/testdata/voice-live/multi-turn.json"
fallback_fixture="${TAU_DEMO_REPO_ROOT}/crates/tau-voice/testdata/voice-live/fallbacks.json"
demo_state_dir="${TAU_DEMO_VOICE_LIVE_STATE_DIR:-.tau/demo-voice-live}"

if [[ "${demo_state_dir}" = /* ]]; then
  demo_state_path="${demo_state_dir}"
else
  demo_state_path="${TAU_DEMO_REPO_ROOT}/${demo_state_dir}"
fi
artifact_manifest_path="${demo_state_path}/artifact-manifest.json"

tau_demo_common_require_file "${single_turn_fixture}"
tau_demo_common_require_file "${multi_turn_fixture}"
tau_demo_common_require_file "${fallback_fixture}"
tau_demo_common_require_command python3
tau_demo_common_prepare_binary

rm -rf "${demo_state_path}"

tau_demo_common_run_step \
  "voice-live-runner-single-turn" \
  --voice-live-runner \
  --voice-live-input ./crates/tau-coding-agent/testdata/voice-live/single-turn.json \
  --voice-state-dir "${demo_state_dir}" \
  --voice-live-wake-word tau \
  --voice-live-max-turns 64 \
  --voice-live-tts-output

tau_demo_common_run_step \
  "voice-live-runner-multi-turn" \
  --voice-live-runner \
  --voice-live-input ./crates/tau-voice/testdata/voice-live/multi-turn.json \
  --voice-state-dir "${demo_state_dir}" \
  --voice-live-wake-word tau \
  --voice-live-max-turns 64 \
  --voice-live-tts-output=false

tau_demo_common_run_step \
  "voice-live-runner-fallbacks" \
  --voice-live-runner \
  --voice-live-input ./crates/tau-voice/testdata/voice-live/fallbacks.json \
  --voice-state-dir "${demo_state_dir}" \
  --voice-live-wake-word tau \
  --voice-live-max-turns 64 \
  --voice-live-tts-output

tau_demo_common_run_step \
  "transport-health-inspect-voice-live" \
  --voice-state-dir "${demo_state_dir}" \
  --transport-health-inspect voice \
  --transport-health-json

tau_demo_common_run_step \
  "voice-status-inspect-live" \
  --voice-state-dir "${demo_state_dir}" \
  --voice-status-inspect \
  --voice-status-json

tau_demo_common_run_step \
  "channel-store-inspect-voice-live-ops-live" \
  --channel-store-root "${demo_state_dir}/channel-store" \
  --channel-store-inspect voice/ops-live

python3 - "${demo_state_path}" "${artifact_manifest_path}" "${demo_state_dir}" <<'PY'
import json
import os
import sys
import time
from pathlib import Path

state_dir = Path(sys.argv[1])
manifest_path = Path(sys.argv[2])
state_dir_label = sys.argv[3]

def artifact(name: str, path: Path) -> dict:
    return {
        "name": name,
        "path": str(path),
        "exists": path.exists(),
        "size_bytes": path.stat().st_size if path.exists() else 0,
    }

state_path = state_dir / "state.json"
events_path = state_dir / "runtime-events.jsonl"
channel_store_path = state_dir / "channel-store"

artifacts = [
    artifact("state", state_path),
    artifact("runtime_events", events_path),
    artifact("channel_store_root", channel_store_path),
]

trace_log = os.environ.get("TAU_DEMO_TRACE_LOG", "").strip()
if trace_log:
    trace_path = Path(trace_log)
    artifacts.append(artifact("trace_log", trace_path))

health_snapshot = {}
if state_path.exists():
    try:
        parsed_state = json.loads(state_path.read_text(encoding="utf-8"))
        health_snapshot = parsed_state.get("health", {})
    except json.JSONDecodeError:
        health_snapshot = {"parse_error": "invalid_state_json"}

last_reason_codes = []
last_health_state = ""
if events_path.exists():
    lines = [line for line in events_path.read_text(encoding="utf-8").splitlines() if line.strip()]
    if lines:
        try:
            last_event = json.loads(lines[-1])
            codes = last_event.get("reason_codes", [])
            if isinstance(codes, list):
                last_reason_codes = [str(code) for code in codes]
            last_health_state = str(last_event.get("health_state", ""))
        except json.JSONDecodeError:
            last_reason_codes = ["invalid_runtime_events_json"]

payload = {
    "schema_version": 1,
    "demo": "voice-live",
    "generated_unix_ms": int(time.time() * 1000),
    "state_dir": state_dir_label,
    "artifacts": artifacts,
    "last_health_state": last_health_state,
    "last_reason_codes": last_reason_codes,
    "health_snapshot": health_snapshot,
}

manifest_path.parent.mkdir(parents=True, exist_ok=True)
manifest_path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
print(f"[demo:voice-live] artifact-manifest: {manifest_path}")
PY

tau_demo_common_require_file "${artifact_manifest_path}"
tau_demo_common_finish
