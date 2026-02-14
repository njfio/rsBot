#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
binary_path="${repo_root}/target/debug/tau-coding-agent"
skip_build="false"
timeout_seconds=""
step_total=0
step_passed=0

print_usage() {
  cat <<EOF
Usage: custom-command-live.sh [--repo-root PATH] [--binary PATH] [--skip-build] [--timeout-seconds N] [--help]

Run deterministic custom-command live proof flow with summary/report artifact generation.

Options:
  --repo-root PATH      Repository root (defaults to caller-derived root)
  --binary PATH         tau-coding-agent binary path
  --skip-build          Skip cargo build and require binary to exist
  --timeout-seconds N   Positive integer timeout per command step
  --help                Show this usage message
EOF
}

log_info() {
  echo "[demo:custom-command-live] $1"
}

run_step() {
  local label="$1"
  shift
  step_total=$((step_total + 1))
  log_info "[${step_total}] ${label}"
  if "$@"; then
    step_passed=$((step_passed + 1))
    log_info "PASS ${label}"
  else
    local rc=$?
    log_info "FAIL ${label} exit=${rc}"
    return "${rc}"
  fi
}

run_with_timeout() {
  local -a command=("$@")
  if [[ -n "${timeout_seconds}" ]]; then
    python3 - "${timeout_seconds}" "${command[@]}" <<'PY'
import subprocess
import sys

timeout_seconds = int(sys.argv[1])
command = sys.argv[2:]
try:
    completed = subprocess.run(command, timeout=timeout_seconds)
except subprocess.TimeoutExpired:
    sys.exit(124)
sys.exit(completed.returncode)
PY
  else
    "${command[@]}"
  fi
}

run_logged_command() {
  local stdout_log="$1"
  local stderr_log="$2"
  shift 2
  set +e
  run_with_timeout "$@" >"${stdout_log}" 2>"${stderr_log}"
  local rc=$?
  set -e
  return "${rc}"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo-root)
      if [[ $# -lt 2 ]]; then
        echo "missing value for --repo-root" >&2
        print_usage >&2
        exit 2
      fi
      repo_root="$2"
      shift 2
      ;;
    --binary)
      if [[ $# -lt 2 ]]; then
        echo "missing value for --binary" >&2
        print_usage >&2
        exit 2
      fi
      binary_path="$2"
      shift 2
      ;;
    --skip-build)
      skip_build="true"
      shift
      ;;
    --timeout-seconds)
      if [[ $# -lt 2 ]]; then
        echo "missing value for --timeout-seconds" >&2
        print_usage >&2
        exit 2
      fi
      if [[ ! "$2" =~ ^[1-9][0-9]*$ ]]; then
        echo "invalid value for --timeout-seconds (expected positive integer): $2" >&2
        print_usage >&2
        exit 2
      fi
      timeout_seconds="$2"
      shift 2
      ;;
    --help)
      print_usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      print_usage >&2
      exit 2
      ;;
  esac
done

if [[ ! -d "${repo_root}" ]]; then
  echo "invalid --repo-root path (directory not found): ${repo_root}" >&2
  exit 2
fi
repo_root="$(cd "${repo_root}" && pwd)"

if [[ "${binary_path}" != /* ]]; then
  binary_path="${repo_root}/${binary_path}"
fi

fixture_path="${repo_root}/crates/tau-coding-agent/testdata/custom-command-contract/live-execution-matrix.json"
if [[ ! -f "${fixture_path}" ]]; then
  echo "missing required custom-command fixture: ${fixture_path}" >&2
  exit 1
fi

work_root="${repo_root}/.tau/demo-custom-command-live"
state_dir="${work_root}/state"
summary_json="${work_root}/custom-command-live-summary.json"
report_json="${work_root}/custom-command-live-report.json"
transcript_log="${work_root}/custom-command-live-transcript.log"

rm -rf "${work_root}"
mkdir -p "${state_dir}"

if [[ "${skip_build}" != "true" ]]; then
  run_step "build-tau-coding-agent" \
    bash -lc "cd '${repo_root}' && cargo build -p tau-coding-agent >/dev/null"
fi

if [[ ! -x "${binary_path}" ]]; then
  echo "missing tau-coding-agent binary: ${binary_path}" >&2
  exit 1
fi

run_step "custom-command-live-runner" \
  run_logged_command \
  "${work_root}/runner.stdout.log" \
  "${work_root}/runner.stderr.log" \
  "${binary_path}" \
  --custom-command-contract-runner \
  --custom-command-fixture "${fixture_path}" \
  --custom-command-state-dir "${state_dir}" \
  --custom-command-queue-limit 64 \
  --custom-command-processed-case-cap 10000 \
  --custom-command-retry-max-attempts 2 \
  --custom-command-retry-base-delay-ms 0

run_step "transport-health-inspect-custom-command" \
  run_logged_command \
  "${work_root}/health.stdout.log" \
  "${work_root}/health.stderr.log" \
  "${binary_path}" \
  --custom-command-state-dir "${state_dir}" \
  --transport-health-inspect custom-command \
  --transport-health-json

run_step "custom-command-status-inspect" \
  run_logged_command \
  "${work_root}/status.stdout.log" \
  "${work_root}/status.stderr.log" \
  "${binary_path}" \
  --custom-command-state-dir "${state_dir}" \
  --custom-command-status-inspect \
  --custom-command-status-json

run_step "channel-store-inspect-custom-command-deploy-release" \
  run_logged_command \
  "${work_root}/deploy-release.stdout.log" \
  "${work_root}/deploy-release.stderr.log" \
  "${binary_path}" \
  --channel-store-root "${state_dir}/channel-store" \
  --channel-store-inspect custom-command/deploy_release

run_step "channel-store-inspect-custom-command-admin-shutdown" \
  run_logged_command \
  "${work_root}/admin-shutdown.stdout.log" \
  "${work_root}/admin-shutdown.stderr.log" \
  "${binary_path}" \
  --channel-store-root "${state_dir}/channel-store" \
  --channel-store-inspect custom-command/admin_shutdown

run_step "channel-store-inspect-custom-command-triage-alerts" \
  run_logged_command \
  "${work_root}/triage-alerts.stdout.log" \
  "${work_root}/triage-alerts.stderr.log" \
  "${binary_path}" \
  --channel-store-root "${state_dir}/channel-store" \
  --channel-store-inspect custom-command/triage_alerts

validate_outputs() {
  python3 - \
    "${state_dir}" \
    "${work_root}" \
    "${summary_json}" \
    "${report_json}" \
    "${transcript_log}" <<'PY'
import json
import sys
from pathlib import Path

state_dir = Path(sys.argv[1])
work_root = Path(sys.argv[2])
summary_path = Path(sys.argv[3])
report_path = Path(sys.argv[4])
transcript_path = Path(sys.argv[5])

state_path = state_dir / "state.json"
if not state_path.exists():
    raise SystemExit(f"custom-command state missing: {state_path}")
state_payload = json.loads(state_path.read_text(encoding="utf-8"))

health_stdout = work_root / "health.stdout.log"
status_stdout = work_root / "status.stdout.log"
if not health_stdout.exists() or not status_stdout.exists():
    raise SystemExit("health/status inspection logs are missing")

raw_health_payload = json.loads(health_stdout.read_text(encoding="utf-8"))
status_payload = json.loads(status_stdout.read_text(encoding="utf-8"))
if not isinstance(status_payload, dict):
    raise SystemExit("custom-command status inspect payload must be an object")

if isinstance(raw_health_payload, list):
    first_entry = raw_health_payload[0] if raw_health_payload else {}
    if not isinstance(first_entry, dict):
        raise SystemExit("transport health inspect list payload must contain objects")
    health_payload = first_entry.get("health", {})
    if not isinstance(health_payload, dict):
        raise SystemExit("transport health inspect entry has invalid health payload")
    health_state = status_payload.get("health_state", "")
elif isinstance(raw_health_payload, dict):
    if isinstance(raw_health_payload.get("health"), dict):
        health_payload = raw_health_payload.get("health", {})
    else:
        health_payload = raw_health_payload
    health_state = raw_health_payload.get(
        "health_state",
        status_payload.get("health_state", ""),
    )
else:
    raise SystemExit("transport health inspect payload must be an object or array")

channel_logs = sorted((state_dir / "channel-store" / "channels" / "custom-command").glob("*/log.jsonl"))
if not channel_logs:
    raise SystemExit("custom-command channel-store logs are missing")

deploy_events = []
policy_deny_events = []
retryable_failure_events = []

for log_file in channel_logs:
    lines = [line for line in log_file.read_text(encoding="utf-8").splitlines() if line.strip()]
    for line in lines:
        event = json.loads(line)
        payload = event.get("payload", {})
        if not isinstance(payload, dict):
            continue
        if payload.get("command_name") == "deploy_release":
            deploy_events.append(event)
        if payload.get("error_code") == "custom_command_policy_denied":
            policy_deny_events.append(event)
        if payload.get("error_code") == "custom_command_backend_unavailable":
            retryable_failure_events.append(event)

if not deploy_events:
    raise SystemExit("deploy_release execution events were not captured")
if not policy_deny_events:
    raise SystemExit("policy deny events were not captured")
if not retryable_failure_events:
    raise SystemExit("retryable failure events were not captured")

runner_stdout = work_root / "runner.stdout.log"
runner_stderr = work_root / "runner.stderr.log"
segments = []
for log_path in (
    runner_stdout,
    runner_stderr,
    health_stdout,
    work_root / "health.stderr.log",
    status_stdout,
    work_root / "status.stderr.log",
    work_root / "deploy-release.stdout.log",
    work_root / "deploy-release.stderr.log",
    work_root / "admin-shutdown.stdout.log",
    work_root / "admin-shutdown.stderr.log",
    work_root / "triage-alerts.stdout.log",
    work_root / "triage-alerts.stderr.log",
):
    if log_path.exists():
        segments.append(f"### {log_path.name}")
        segments.append(log_path.read_text(encoding="utf-8"))
transcript_path.write_text("\n".join(segments), encoding="utf-8")

summary = {
    "schema_version": 1,
    "health_state": health_state,
    "rollout_gate": status_payload.get("rollout_gate", ""),
    "failure_streak": int(health_payload.get("failure_streak", 0)),
    "queue_depth": int(health_payload.get("queue_depth", 0)),
    "commands_remaining": len(state_payload.get("commands", [])),
    "deploy_event_count": len(deploy_events),
    "policy_deny_event_count": len(policy_deny_events),
    "retryable_failure_event_count": len(retryable_failure_events),
}
summary_path.write_text(json.dumps(summary, indent=2), encoding="utf-8")

report = {
    "schema_version": 1,
    "summary_path": str(summary_path),
    "transcript_path": str(transcript_path),
    "state_path": str(state_path),
    "health_inspect_path": str(health_stdout),
    "status_inspect_path": str(status_stdout),
    "channel_log_paths": [str(path) for path in channel_logs],
    "summary": summary,
}
report_path.write_text(json.dumps(report, indent=2), encoding="utf-8")
print(f"report_json={report_path}")
PY
}

run_step "validate-custom-command-live-artifacts-and-write-report" validate_outputs

log_info "summary_json=${summary_json}"
log_info "report_json=${report_json}"
log_info "transcript_log=${transcript_log}"
log_info "summary: total=${step_total} passed=${step_passed} failed=$((step_total - step_passed))"
