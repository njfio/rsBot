#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
skip_build="false"
timeout_seconds=""
unused_binary=""
harness_bin=""
workspace_id="demo-workspace"
step_total=0
step_passed=0

print_usage() {
  cat <<EOF
Usage: memory-live.sh [--repo-root PATH] [--binary PATH] [--harness-bin PATH] [--workspace-id VALUE] [--skip-build] [--timeout-seconds N] [--help]

Run deterministic live memory backend proof and emit summary/quality/artifact-manifest JSON outputs.

Options:
  --repo-root PATH      Repository root (defaults to caller-derived root)
  --binary PATH         Accepted for wrapper compatibility; unused by this script
  --harness-bin PATH    memory_live_harness binary path
  --workspace-id VALUE  Workspace id used for persisted memory partitioning
  --skip-build          Skip cargo build and use existing harness binary
  --timeout-seconds N   Positive integer timeout for harness execution
  --help                Show this usage message
EOF
}

log_info() {
  echo "[demo:memory-live] $1"
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
      unused_binary="$2"
      shift 2
      ;;
    --harness-bin)
      if [[ $# -lt 2 ]]; then
        echo "missing value for --harness-bin" >&2
        print_usage >&2
        exit 2
      fi
      harness_bin="$2"
      shift 2
      ;;
    --workspace-id)
      if [[ $# -lt 2 ]]; then
        echo "missing value for --workspace-id" >&2
        print_usage >&2
        exit 2
      fi
      workspace_id="$2"
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

if [[ -z "${harness_bin}" ]]; then
  harness_bin="${repo_root}/target/debug/memory_live_harness"
elif [[ "${harness_bin}" != /* ]]; then
  harness_bin="${repo_root}/${harness_bin}"
fi

work_root="${repo_root}/.tau/demo-memory-live"
state_dir="${work_root}/state"
summary_json="${work_root}/memory-live-summary.json"
quality_json="${work_root}/memory-live-quality-report.json"
manifest_json="${work_root}/memory-live-artifact-manifest.json"
report_json="${work_root}/memory-live-report.json"
transcript_log="${work_root}/memory-live-transcript.log"

rm -rf "${work_root}"
mkdir -p "${work_root}" "${state_dir}"

if [[ "${skip_build}" != "true" ]]; then
  run_step "build-memory-live-harness" \
    bash -lc "cd '${repo_root}' && cargo build -p tau-agent-core --bin memory_live_harness >/dev/null"
fi

if [[ ! -x "${harness_bin}" ]]; then
  echo "missing harness binary: ${harness_bin}" >&2
  exit 1
fi

run_harness() {
  local -a command=(
    "${harness_bin}"
    "--output-dir" "${work_root}"
    "--state-dir" "${state_dir}"
    "--summary-json-out" "${summary_json}"
    "--quality-report-json-out" "${quality_json}"
    "--artifact-manifest-json-out" "${manifest_json}"
    "--workspace-id" "${workspace_id}"
  )

  set +e
  run_with_timeout "${command[@]}" 2>&1 | tee "${transcript_log}"
  local rc=${PIPESTATUS[0]}
  set -e
  return "${rc}"
}

run_step "run-memory-live-harness" run_harness

for required_file in "${summary_json}" "${quality_json}" "${manifest_json}" "${transcript_log}"; do
  if [[ ! -f "${required_file}" ]]; then
    echo "missing expected output artifact: ${required_file}" >&2
    exit 1
  fi
done

run_step "synthesize-memory-live-report" \
  python3 - "${summary_json}" "${quality_json}" "${manifest_json}" "${report_json}" <<'PY'
import json
import sys
from pathlib import Path

summary_path = Path(sys.argv[1])
quality_path = Path(sys.argv[2])
manifest_path = Path(sys.argv[3])
report_path = Path(sys.argv[4])

summary = json.loads(summary_path.read_text(encoding="utf-8"))
quality = json.loads(quality_path.read_text(encoding="utf-8"))
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))

quality_gate_passed = bool(summary.get("quality_gate_passed")) and bool(
    quality.get("metrics", {}).get("quality_gate_passed")
)
if not quality_gate_passed:
    print("memory live quality gate failed", file=sys.stderr)
    sys.exit(1)

report = {
    "schema_version": 1,
    "quality_gate_passed": quality_gate_passed,
    "workspace_id": summary.get("workspace_id"),
    "total_cases": summary.get("total_cases"),
    "top1_relevance_rate": summary.get("top1_relevance_rate"),
    "topk_relevance_rate": summary.get("topk_relevance_rate"),
    "persisted_entry_count": summary.get("persisted_entry_count"),
    "quality_thresholds": quality.get("thresholds", {}),
    "quality_metrics": quality.get("metrics", {}),
    "artifact_manifest_entries": len(manifest.get("artifacts", [])),
    "summary_path": str(summary_path),
    "quality_report_path": str(quality_path),
    "artifact_manifest_path": str(manifest_path),
}

report_path.write_text(json.dumps(report, indent=2), encoding="utf-8")
print(
    f"report quality gate passed with top1={report['top1_relevance_rate']} topk={report['topk_relevance_rate']}"
)
PY

run_step "print-memory-live-report" \
  python3 - "${report_json}" <<'PY'
import json
import sys
from pathlib import Path

report = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
print(json.dumps(report, indent=2))
PY

failed=$((step_total - step_passed))
log_info "summary: total=${step_total} passed=${step_passed} failed=${failed}"

