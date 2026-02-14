#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
binary="${repo_root}/target/debug/tau-coding-agent"
skip_build="false"
list_only="false"
json_output="false"
has_only_filter="false"
report_file=""
fail_fast="false"
timeout_seconds=""

demo_scripts=(
  "local.sh"
  "rpc.sh"
  "events.sh"
  "package.sh"
  "multi-channel.sh"
  "multi-agent.sh"
  "browser-automation.sh"
  "browser-automation-live.sh"
  "memory.sh"
  "dashboard.sh"
  "gateway.sh"
  "gateway-auth.sh"
  "gateway-remote-access.sh"
  "deployment.sh"
  "custom-command.sh"
  "voice.sh"
)

declare -A selected_demo_lookup=()
declare -a unknown_demo_names=()

log_info() {
  local message="$1"
  if [[ "${json_output}" == "true" ]]; then
    echo "${message}" >&2
  else
    echo "${message}"
  fi
}

log_error() {
  local message="$1"
  echo "${message}" >&2
}

require_command() {
  local executable="$1"
  if ! command -v "${executable}" >/dev/null 2>&1; then
    log_error "missing required executable: ${executable}"
    return 1
  fi
}

trim_spaces() {
  local value="$1"
  value="${value#"${value%%[![:space:]]*}"}"
  value="${value%"${value##*[![:space:]]}"}"
  echo "${value}"
}

normalize_demo_name() {
  local candidate="$1"
  case "${candidate}" in
    local|local.sh)
      echo "local.sh"
      return 0
      ;;
    rpc|rpc.sh)
      echo "rpc.sh"
      return 0
      ;;
    events|events.sh)
      echo "events.sh"
      return 0
      ;;
    package|package.sh)
      echo "package.sh"
      return 0
      ;;
    multi-channel|multichannel|multi-channel.sh|multichannel.sh)
      echo "multi-channel.sh"
      return 0
      ;;
    multi-agent|multiagent|multi-agent.sh|multiagent.sh)
      echo "multi-agent.sh"
      return 0
      ;;
    browser-automation|browserautomation|browser|browser-automation.sh|browserautomation.sh|browser.sh)
      echo "browser-automation.sh"
      return 0
      ;;
    browser-automation-live|browserautomationlive|browser-live|browser-automation-live.sh|browserautomationlive.sh|browser-live.sh)
      echo "browser-automation-live.sh"
      return 0
      ;;
    memory|memory.sh)
      echo "memory.sh"
      return 0
      ;;
    dashboard|dashboard.sh)
      echo "dashboard.sh"
      return 0
      ;;
    gateway|gateway.sh)
      echo "gateway.sh"
      return 0
      ;;
    gateway-auth|gatewayauth|gateway-auth.sh|gatewayauth.sh)
      echo "gateway-auth.sh"
      return 0
      ;;
    gateway-remote-access|gatewayremoteaccess|gateway-remote-access.sh|gatewayremoteaccess.sh)
      echo "gateway-remote-access.sh"
      return 0
      ;;
    deployment|deployment.sh)
      echo "deployment.sh"
      return 0
      ;;
    custom-command|customcommand|custom-command.sh|customcommand.sh)
      echo "custom-command.sh"
      return 0
      ;;
    voice|voice.sh)
      echo "voice.sh"
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

print_demo_list_text() {
  local selected=("$@")
  local demo_script
  for demo_script in "${selected[@]}"; do
    echo "${demo_script}"
  done
}

print_demo_list_json() {
  local selected=("$@")
  local idx
  echo "{"
  printf '  "demos": ['
  for idx in "${!selected[@]}"; do
    if [[ "${idx}" -gt 0 ]]; then
      printf ', '
    fi
    printf '"%s"' "${selected[$idx]}"
  done
  echo "]"
  echo "}"
}

run_demo_names=()
run_demo_statuses=()
run_demo_exit_codes=()
run_demo_durations_ms=()

current_time_ms() {
  if command -v python3 >/dev/null 2>&1; then
    python3 - <<'PY'
import time
print(int(time.time() * 1000))
PY
    return 0
  fi

  date +%s | awk '{ print $1 * 1000 }'
}

write_report_file() {
  local payload="$1"
  local destination="$2"
  local destination_parent
  destination_parent="$(dirname "${destination}")"
  mkdir -p "${destination_parent}"
  printf '%s\n' "${payload}" > "${destination}"
}

prepare_binary_once() {
  if [[ "${skip_build}" == "true" ]]; then
    if [[ ! -f "${binary}" ]]; then
      log_error "missing tau-coding-agent binary (use --binary or remove --skip-build): ${binary}"
      return 1
    fi
    return 0
  fi

  require_command cargo || return 1
  log_info "[demo:all] building tau-coding-agent"
  (
    cd "${repo_root}"
    cargo build -p tau-coding-agent >/dev/null
  ) || return $?

  if [[ ! -f "${binary}" ]]; then
    log_error "missing tau-coding-agent binary after build: ${binary}"
    return 1
  fi
}

print_summary_json() {
  local total_count="$1"
  local passed_count="$2"
  local failed_count="$3"
  local idx
  local last_index

  echo "{"
  echo "  \"demos\": ["
  if [[ ${#run_demo_names[@]} -gt 0 ]]; then
    last_index=$(( ${#run_demo_names[@]} - 1 ))
    for idx in "${!run_demo_names[@]}"; do
      comma=","
      if [[ "${idx}" -eq "${last_index}" ]]; then
        comma=""
      fi
      printf '    {"name":"%s","status":"%s","exit_code":%s,"duration_ms":%s}%s\n' \
        "${run_demo_names[$idx]}" \
        "${run_demo_statuses[$idx]}" \
        "${run_demo_exit_codes[$idx]}" \
        "${run_demo_durations_ms[$idx]}" \
        "${comma}"
    done
  fi
  echo "  ],"
  printf '  "summary":{"total":%s,"passed":%s,"failed":%s}\n' "${total_count}" "${passed_count}" "${failed_count}"
  echo "}"
}

print_usage() {
  cat <<EOF
Usage: all.sh [--repo-root PATH] [--binary PATH] [--skip-build] [--list] [--only DEMOS] [--json] [--report-file PATH] [--fail-fast] [--timeout-seconds N] [--help]

Run checked-in Tau demo wrappers (local/rpc/events/package/multi-channel/multi-agent/browser-automation/browser-automation-live/memory/dashboard/gateway/gateway-auth/gateway-remote-access/deployment/custom-command/voice) with deterministic summary output.

Options:
  --repo-root PATH  Repository root (defaults to caller-derived root)
  --binary PATH     tau-coding-agent binary path (default: <repo-root>/target/debug/tau-coding-agent)
  --skip-build      Skip cargo build and require --binary to exist
  --list            Print selected demos and exit without execution
  --only DEMOS      Comma-separated subset (names: local,rpc,events,package,multi-channel,multi-agent,browser-automation,browser-automation-live,memory,dashboard,gateway,gateway-auth,gateway-remote-access,deployment,custom-command,voice)
  --json            Emit deterministic JSON output for list/summary modes
  --report-file     Write deterministic JSON report artifact to path
  --fail-fast       Stop after first failed wrapper
  --timeout-seconds Positive integer timeout for each wrapper step
  --help            Show this usage message
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo-root)
      if [[ $# -lt 2 ]]; then
        log_error "missing value for --repo-root"
        print_usage >&2
        exit 2
      fi
      repo_root="$2"
      shift 2
      ;;
    --binary)
      if [[ $# -lt 2 ]]; then
        log_error "missing value for --binary"
        print_usage >&2
        exit 2
      fi
      binary="$2"
      shift 2
      ;;
    --skip-build)
      skip_build="true"
      shift
      ;;
    --list)
      list_only="true"
      shift
      ;;
    --only)
      if [[ $# -lt 2 || -z "$2" ]]; then
        log_error "missing value for --only"
        print_usage >&2
        exit 2
      fi
      has_only_filter="true"
      IFS=',' read -r -a requested_demo_names <<< "$2"
      for requested_demo_name in "${requested_demo_names[@]}"; do
        trimmed_requested_demo_name="$(trim_spaces "${requested_demo_name}")"
        if [[ -z "${trimmed_requested_demo_name}" ]]; then
          continue
        fi
        if normalized_demo_name="$(normalize_demo_name "${trimmed_requested_demo_name}")"; then
          selected_demo_lookup["${normalized_demo_name}"]=1
        else
          unknown_demo_names+=("${trimmed_requested_demo_name}")
        fi
      done
      shift 2
      ;;
    --json)
      json_output="true"
      shift
      ;;
    --report-file)
      if [[ $# -lt 2 || -z "$2" ]]; then
        log_error "missing value for --report-file"
        print_usage >&2
        exit 2
      fi
      report_file="$2"
      shift 2
      ;;
    --fail-fast)
      fail_fast="true"
      shift
      ;;
    --timeout-seconds)
      if [[ $# -lt 2 ]]; then
        log_error "missing value for --timeout-seconds"
        print_usage >&2
        exit 2
      fi
      if [[ ! "$2" =~ ^[1-9][0-9]*$ ]]; then
        log_error "invalid value for --timeout-seconds (expected positive integer): $2"
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
      log_error "unknown argument: $1"
      print_usage >&2
      exit 2
      ;;
  esac
done

if [[ ! -d "${repo_root}" ]]; then
  log_error "invalid --repo-root path (directory not found): ${repo_root}"
  exit 2
fi
repo_root="$(cd "${repo_root}" && pwd)"

if [[ "${binary}" != /* ]]; then
  binary="${repo_root}/${binary}"
fi

if [[ ${#unknown_demo_names[@]} -gt 0 ]]; then
  log_error "unknown demo names in --only: ${unknown_demo_names[*]}"
  exit 2
fi

selected_demo_scripts=("${demo_scripts[@]}")
if [[ "${has_only_filter}" == "true" ]]; then
  selected_demo_scripts=()
  for demo_script in "${demo_scripts[@]}"; do
    if [[ -n "${selected_demo_lookup[${demo_script}]:-}" ]]; then
      selected_demo_scripts+=("${demo_script}")
    fi
  done
  if [[ ${#selected_demo_scripts[@]} -eq 0 ]]; then
    log_error "no demos selected by --only filter"
    exit 2
  fi
fi

if [[ "${list_only}" == "true" ]]; then
  list_json_payload="$(print_demo_list_json "${selected_demo_scripts[@]}")"
  if [[ -n "${report_file}" ]]; then
    write_report_file "${list_json_payload}" "${report_file}"
  fi
  if [[ "${json_output}" == "true" ]]; then
    echo "${list_json_payload}"
  else
    print_demo_list_text "${selected_demo_scripts[@]}"
  fi
  exit 0
fi

prepare_binary_once || exit $?

total=0
passed=0
failed=0

for demo_script in "${selected_demo_scripts[@]}"; do
  total=$((total + 1))
  log_info "[demo:all] [${total}] ${demo_script}"
  args=("${script_dir}/${demo_script}" "--repo-root" "${repo_root}" "--binary" "${binary}" "--skip-build")
  if [[ -n "${timeout_seconds}" ]]; then
    args+=("--timeout-seconds" "${timeout_seconds}")
  fi
  started_ms="$(current_time_ms)"

  if [[ "${json_output}" == "true" ]]; then
    if "${args[@]}" >&2; then
      demo_exit_code=0
      demo_status="passed"
    else
      demo_exit_code=$?
      demo_status="failed"
    fi
  elif "${args[@]}"; then
    demo_exit_code=0
    demo_status="passed"
  else
    demo_exit_code=$?
    demo_status="failed"
  fi
  ended_ms="$(current_time_ms)"
  demo_duration_ms=$((ended_ms - started_ms))
  if [[ "${demo_duration_ms}" -lt 0 ]]; then
    demo_duration_ms=0
  fi

  run_demo_names+=("${demo_script}")
  run_demo_statuses+=("${demo_status}")
  run_demo_exit_codes+=("${demo_exit_code}")
  run_demo_durations_ms+=("${demo_duration_ms}")

  if [[ "${demo_status}" == "passed" ]]; then
    passed=$((passed + 1))
    log_info "[demo:all] PASS ${demo_script}"
  else
    failed=$((failed + 1))
    log_error "[demo:all] FAIL ${demo_script}"
    if [[ "${fail_fast}" == "true" ]]; then
      log_error "[demo:all] fail-fast triggered; stopping after ${demo_script}"
      break
    fi
  fi
done

summary_json_payload="$(print_summary_json "${total}" "${passed}" "${failed}")"
if [[ -n "${report_file}" ]]; then
  write_report_file "${summary_json_payload}" "${report_file}"
fi

if [[ "${json_output}" == "true" ]]; then
  echo "${summary_json_payload}"
else
  echo "[demo:all] summary: total=${total} passed=${passed} failed=${failed}"
fi

if [[ ${failed} -gt 0 ]]; then
  exit 1
fi
