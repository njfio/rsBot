#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
skip_build="false"
timeout_seconds=""
playwright_cli_override=""
unused_binary=""
step_total=0
step_passed=0

print_usage() {
  cat <<EOF
Usage: browser-automation-live.sh [--repo-root PATH] [--binary PATH] [--skip-build] [--timeout-seconds N] [--playwright-cli PATH] [--help]

Run deterministic browser live-harness validation and emit machine-readable summary/report artifacts.

Options:
  --repo-root PATH      Repository root (defaults to caller-derived root)
  --binary PATH         Accepted for wrapper compatibility; unused by this script
  --skip-build          Skip cargo build and use existing harness binary
  --timeout-seconds N   Positive integer timeout for the live harness execution step
  --playwright-cli PATH Use this Playwright CLI path instead of the deterministic mock fallback
  --help                Show this usage message
EOF
}

log_info() {
  echo "[demo:browser-automation-live] $1"
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
    --playwright-cli)
      if [[ $# -lt 2 ]]; then
        echo "missing value for --playwright-cli" >&2
        print_usage >&2
        exit 2
      fi
      playwright_cli_override="$2"
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

work_root="${repo_root}/.tau/demo-browser-automation-live"
work_dir="${work_root}/work"
state_dir="${work_root}/state"
fixture_path="${work_dir}/browser-live-fixture.json"
page_path="${work_dir}/browser-live-page.html"
summary_json="${work_root}/browser-live-summary.json"
report_json="${work_root}/browser-live-report.json"
transcript_log="${work_root}/browser-live-transcript.log"
mock_cli_path="${work_dir}/mock-playwright-cli.py"
harness_bin="${repo_root}/target/debug/browser_automation_live_harness"

rm -rf "${work_root}"
mkdir -p "${work_dir}" "${state_dir}"

cat >"${page_path}" <<'EOF'
<html><head><title>Demo Live Page</title></head><body><button id='run'>Run</button></body></html>
EOF

cat >"${fixture_path}" <<EOF
{
  "schema_version": 1,
  "name": "demo-browser-live",
  "description": "Deterministic live browser harness fixture for CI/local proof runs",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "navigate-demo",
      "operation": "navigate",
      "url": "file://${page_path}",
      "expected": {
        "outcome": "success",
        "status_code": 200,
        "response_body": {
          "status": "ok",
          "operation": "navigate",
          "url": "file://${page_path}",
          "title": "Demo Live Page",
          "dom_nodes": 10
        }
      }
    },
    {
      "schema_version": 1,
      "case_id": "snapshot-demo",
      "operation": "snapshot",
      "expected": {
        "outcome": "success",
        "status_code": 200,
        "response_body": {
          "status": "ok",
          "operation": "snapshot",
          "snapshot_id": "snapshot-live",
          "elements": [{"id": "e1", "role": "button", "name": "Run"}]
        }
      }
    },
    {
      "schema_version": 1,
      "case_id": "action-demo",
      "operation": "action",
      "action": "click",
      "selector": "#run",
      "text": "go",
      "timeout_ms": 1000,
      "expected": {
        "outcome": "success",
        "status_code": 200,
        "response_body": {
          "status": "ok",
          "operation": "action",
          "action": "click",
          "selector": "#run",
          "repeat_count": 1,
          "text": "go",
          "timeout_ms": 1000
        }
      }
    }
  ]
}
EOF

if [[ -n "${playwright_cli_override}" ]]; then
  playwright_cli="${playwright_cli_override}"
  if [[ ! -x "${playwright_cli}" ]]; then
    echo "provided --playwright-cli is not executable: ${playwright_cli}" >&2
    exit 2
  fi
else
  cat >"${mock_cli_path}" <<'EOF'
#!/usr/bin/env python3
import json
import pathlib
import re
import sys

session_file = pathlib.Path(__file__).with_suffix(".session")
command = sys.argv[1] if len(sys.argv) > 1 else ""

if command == "start-session":
    session_file.write_text("active", encoding="utf-8")
    print(json.dumps({"status": "ok"}))
    raise SystemExit(0)

if command == "shutdown-session":
    if session_file.exists():
        session_file.unlink()
    print(json.dumps({"status": "ok"}))
    raise SystemExit(0)

if command != "execute-action":
    print("unsupported command", file=sys.stderr)
    raise SystemExit(2)

payload = json.loads(sys.argv[2]) if len(sys.argv) > 2 else {}
operation = payload.get("operation", "")

if operation == "navigate":
    url = payload.get("url", "")
    html_path = pathlib.Path(url[7:])
    html = html_path.read_text(encoding="utf-8")
    match = re.search(r"<title>(.*?)</title>", html, re.IGNORECASE | re.DOTALL)
    title = match.group(1).strip() if match else "Untitled"
    print(json.dumps({
        "status_code": 200,
        "response_body": {
            "status": "ok",
            "operation": "navigate",
            "url": url,
            "title": title,
            "dom_nodes": html.count("<")
        },
        "artifacts": {
            "dom_snapshot_html": html,
            "screenshot_svg": "<svg xmlns='http://www.w3.org/2000/svg'><rect width='10' height='10'/></svg>",
            "trace_json": json.dumps({"events": ["navigate"], "url": url})
        }
    }))
    raise SystemExit(0)

if operation == "snapshot":
    print(json.dumps({
        "status_code": 200,
        "response_body": {
            "status": "ok",
            "operation": "snapshot",
            "snapshot_id": "snapshot-live",
            "elements": [{"id": "e1", "role": "button", "name": "Run"}]
        },
        "artifacts": {
            "dom_snapshot_html": "<html><body><button id='run'>Run</button></body></html>",
            "screenshot_svg": "<svg xmlns='http://www.w3.org/2000/svg'><circle cx='5' cy='5' r='5'/></svg>",
            "trace_json": "{\"events\":[\"snapshot\"]}"
        }
    }))
    raise SystemExit(0)

if operation == "action":
    print(json.dumps({
        "status_code": 200,
        "response_body": {
            "status": "ok",
            "operation": "action",
            "action": payload.get("action", ""),
            "selector": payload.get("selector", ""),
            "repeat_count": payload.get("action_repeat_count", 1),
            "text": payload.get("text", ""),
            "timeout_ms": payload.get("timeout_ms", 0)
        },
        "artifacts": {
            "dom_snapshot_html": "",
            "screenshot_svg": "<svg xmlns='http://www.w3.org/2000/svg'><line x1='0' y1='0' x2='10' y2='10'/></svg>",
            "trace_json": "{\"events\":[\"action\"]}"
        }
    }))
    raise SystemExit(0)

print(json.dumps({
    "status_code": 400,
    "error_code": "browser_automation_invalid_operation",
    "response_body": {"status": "rejected", "reason": "invalid_operation"}
}))
EOF
  chmod +x "${mock_cli_path}"
  playwright_cli="${mock_cli_path}"
fi

if [[ "${skip_build}" != "true" ]]; then
  run_step "build-browser-live-harness" \
    bash -lc "cd '${repo_root}' && cargo build -p tau-browser-automation --bin browser_automation_live_harness >/dev/null"
fi

if [[ ! -x "${harness_bin}" ]]; then
  echo "missing harness binary: ${harness_bin}" >&2
  exit 1
fi

run_harness() {
  local -a command=(
    "${harness_bin}"
    "--fixture" "${fixture_path}"
    "--state-dir" "${state_dir}"
    "--playwright-cli" "${playwright_cli}"
    "--summary-json-out" "${summary_json}"
    "--artifact-retention-days" "7"
    "--action-timeout-ms" "4000"
    "--max-actions-per-case" "4"
  )

  set +e
  if [[ -n "${timeout_seconds}" ]]; then
    python3 - "${timeout_seconds}" "${command[@]}" <<'PY' 2>&1 | tee "${transcript_log}"
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
    "${command[@]}" 2>&1 | tee "${transcript_log}"
  fi
  local rc=${PIPESTATUS[0]}
  set -e
  return "${rc}"
}

run_step "run-browser-live-harness" run_harness

validate_summary() {
  python3 - "${summary_json}" "${report_json}" "${transcript_log}" <<'PY'
import json
import sys
from pathlib import Path

summary_path = Path(sys.argv[1])
report_path = Path(sys.argv[2])
transcript_path = Path(sys.argv[3])

if not summary_path.exists():
    raise SystemExit(f"summary JSON missing: {summary_path}")

summary = json.loads(summary_path.read_text(encoding="utf-8"))
if summary.get("discovered_cases") != 3:
    raise SystemExit("unexpected discovered_cases in live summary")
if summary.get("success_cases") != 3:
    raise SystemExit("unexpected success_cases in live summary")
if summary.get("health_state") != "healthy":
    raise SystemExit("live summary health_state is not healthy")
timeline = summary.get("timeline", [])
if len(timeline) != 3:
    raise SystemExit("timeline does not contain all expected cases")

report = {
    "schema_version": 1,
    "summary_path": str(summary_path),
    "transcript_path": str(transcript_path),
    "health_state": summary.get("health_state"),
    "reason_codes": summary.get("reason_codes", []),
    "discovered_cases": summary.get("discovered_cases"),
    "success_cases": summary.get("success_cases"),
    "artifact_records": summary.get("artifact_records"),
    "timeline": timeline,
}
report_path.parent.mkdir(parents=True, exist_ok=True)
report_path.write_text(json.dumps(report, indent=2), encoding="utf-8")
print(f"report_json={report_path}")
PY
}

run_step "validate-live-summary-and-write-report" validate_summary

log_info "summary_json=${summary_json}"
log_info "report_json=${report_json}"
log_info "transcript_log=${transcript_log}"
log_info "summary: total=${step_total} passed=${step_passed} failed=$((step_total - step_passed))"
