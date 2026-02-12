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

scenario_ids=(
  "onboarding"
  "gateway-auth"
  "gateway-remote-access"
  "multi-channel-live"
  "deployment-wasm"
)

declare -A selected_scenario_lookup=()
declare -a unknown_scenario_names=()

run_scenario_names=()
run_scenario_statuses=()
run_scenario_exit_codes=()
run_scenario_durations_ms=()

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

scenario_wrapper() {
  local scenario_id="$1"
  case "${scenario_id}" in
    onboarding)
      echo "local.sh"
      ;;
    gateway-auth)
      echo "gateway-auth.sh"
      ;;
    gateway-remote-access)
      echo "gateway-remote-access.sh"
      ;;
    multi-channel-live)
      echo "multi-channel.sh"
      ;;
    deployment-wasm)
      echo "deployment.sh"
      ;;
    *)
      return 1
      ;;
  esac
}

scenario_description() {
  local scenario_id="$1"
  case "${scenario_id}" in
    onboarding)
      echo "Bootstrap local Tau state and verify first-run operator commands."
      ;;
    gateway-auth)
      echo "Validate gateway remote profile auth posture for token and password-session modes."
      ;;
    gateway-remote-access)
      echo "Validate remote profile inspect + remote plan fail-closed guardrails for wave-9 profiles."
      ;;
    multi-channel-live)
      echo "Exercise Telegram/Discord/WhatsApp fixture ingest and multi-channel routing health."
      ;;
    deployment-wasm)
      echo "Package and inspect deployment WASM artifacts with deterministic status checks."
      ;;
    *)
      return 1
      ;;
  esac
}

scenario_expected_markers() {
  local scenario_id="$1"
  case "${scenario_id}" in
    onboarding)
      echo "[demo:local] PASS onboard-non-interactive"
      echo "[demo:local] summary: total="
      ;;
    gateway-auth)
      echo "[demo:gateway-auth] PASS gateway-remote-profile-token-mode"
      echo "[demo:gateway-auth] PASS gateway-remote-profile-password-session-mode"
      ;;
    gateway-remote-access)
      echo "[demo:gateway-remote-access] PASS gateway-remote-plan-export-tailscale-serve"
      echo "[demo:gateway-remote-access] PASS gateway-remote-plan-fails-closed-for-missing-password"
      ;;
    multi-channel-live)
      echo "[demo:multi-channel] PASS multi-channel-live-ingest-telegram"
      echo "[demo:multi-channel] PASS multi-channel-live-ingest-discord"
      echo "[demo:multi-channel] PASS multi-channel-live-ingest-whatsapp"
      ;;
    deployment-wasm)
      echo "[demo:deployment] PASS deployment-wasm-package"
      echo "[demo:deployment] PASS channel-store-inspect-deployment-edge-wasm"
      ;;
    *)
      return 1
      ;;
  esac
}

scenario_troubleshooting_hint() {
  local scenario_id="$1"
  case "${scenario_id}" in
    onboarding)
      echo "Verify writable .tau state and rerun ./scripts/demo/local.sh --fail-fast to isolate first failing step."
      ;;
    gateway-auth)
      echo "Check gateway auth flags (--gateway-openresponses-auth-mode/token/password) and rerun ./scripts/demo/gateway-auth.sh."
      ;;
    gateway-remote-access)
      echo "Check remote profile/auth flags and inspect .tau/demo-gateway-remote-access/trace.log before rerunning ./scripts/demo/gateway-remote-access.sh."
      ;;
    multi-channel-live)
      echo "Confirm Telegram/Discord/WhatsApp fixture files exist and rerun ./scripts/demo/multi-channel.sh --fail-fast."
      ;;
    deployment-wasm)
      echo "Confirm deployment fixture/module paths exist and rerun ./scripts/demo/deployment.sh --fail-fast."
      ;;
    *)
      return 1
      ;;
  esac
}

normalize_scenario_name() {
  local candidate="$1"
  case "${candidate}" in
    onboarding|local|onboarding.sh|local.sh)
      echo "onboarding"
      return 0
      ;;
    gateway-auth|gatewayauth|gateway-auth.sh|gatewayauth.sh)
      echo "gateway-auth"
      return 0
      ;;
    gateway-remote-access|gatewayremoteaccess|gateway-remote-access.sh|gatewayremoteaccess.sh)
      echo "gateway-remote-access"
      return 0
      ;;
    multi-channel-live|multichannel-live|multi-channel|multi-channel-live.sh|multi-channel.sh)
      echo "multi-channel-live"
      return 0
      ;;
    deployment-wasm|deploymentwasm|deployment|deployment-wasm.sh|deployment.sh)
      echo "deployment-wasm"
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

print_scenario_list_text() {
  local selected=("$@")
  local scenario_id
  local wrapper
  local command_path
  local description
  local hint
  local marker
  local first_marker
  for scenario_id in "${selected[@]}"; do
    wrapper="$(scenario_wrapper "${scenario_id}")"
    command_path="./scripts/demo/${wrapper}"
    description="$(scenario_description "${scenario_id}")"
    hint="$(scenario_troubleshooting_hint "${scenario_id}")"
    echo "${scenario_id}"
    echo "  wrapper: ${wrapper}"
    echo "  command: ${command_path}"
    echo "  description: ${description}"
    first_marker="true"
    while IFS= read -r marker; do
      if [[ "${first_marker}" == "true" ]]; then
        echo "  expected_marker: ${marker}"
        first_marker="false"
      else
        echo "  expected_marker_continued: ${marker}"
      fi
    done < <(scenario_expected_markers "${scenario_id}")
    echo "  troubleshooting: ${hint}"
  done
}

print_scenario_list_json() {
  local selected=("$@")
  local idx
  local marker
  local marker_idx
  local wrapper
  local description
  local hint
  local marker_list
  echo "{"
  echo "  \"schema_version\": 1,"
  echo "  \"scenarios\": ["
  for idx in "${!selected[@]}"; do
    scenario_id="${selected[$idx]}"
    wrapper="$(scenario_wrapper "${scenario_id}")"
    description="$(scenario_description "${scenario_id}")"
    hint="$(scenario_troubleshooting_hint "${scenario_id}")"
    marker_list=()
    while IFS= read -r marker; do
      marker_list+=("${marker}")
    done < <(scenario_expected_markers "${scenario_id}")
    echo "    {"
    echo "      \"id\": \"${scenario_id}\","
    echo "      \"wrapper\": \"${wrapper}\","
    echo "      \"command\": \"./scripts/demo/${wrapper}\","
    echo "      \"description\": \"${description}\","
    echo "      \"expected_markers\": ["
    if [[ ${#marker_list[@]} -gt 0 ]]; then
      for marker_idx in "${!marker_list[@]}"; do
        suffix=","
        if [[ "${marker_idx}" -eq $(( ${#marker_list[@]} - 1 )) ]]; then
          suffix=""
        fi
        echo "        \"${marker_list[$marker_idx]}\"${suffix}"
      done
    fi
    echo "      ],"
    echo "      \"troubleshooting\": \"${hint}\""
    suffix=","
    if [[ "${idx}" -eq $(( ${#selected[@]} - 1 )) ]]; then
      suffix=""
    fi
    echo "    }${suffix}"
  done
  echo "  ]"
  echo "}"
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
  log_info "[demo:index] building tau-coding-agent"
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
  echo "  \"schema_version\": 1,"
  echo "  \"scenarios\": ["
  if [[ ${#run_scenario_names[@]} -gt 0 ]]; then
    last_index=$(( ${#run_scenario_names[@]} - 1 ))
    for idx in "${!run_scenario_names[@]}"; do
      comma=","
      if [[ "${idx}" -eq "${last_index}" ]]; then
        comma=""
      fi
      printf '    {"id":"%s","status":"%s","exit_code":%s,"duration_ms":%s}%s\n' \
        "${run_scenario_names[$idx]}" \
        "${run_scenario_statuses[$idx]}" \
        "${run_scenario_exit_codes[$idx]}" \
        "${run_scenario_durations_ms[$idx]}" \
        "${comma}"
    done
  fi
  echo "  ],"
  printf '  "summary":{"total":%s,"passed":%s,"failed":%s}\n' "${total_count}" "${passed_count}" "${failed_count}"
  echo "}"
}

print_usage() {
  cat <<EOF
Usage: index.sh [--repo-root PATH] [--binary PATH] [--skip-build] [--list] [--only SCENARIOS] [--json] [--report-file PATH] [--fail-fast] [--timeout-seconds N] [--help]

Run the operator-focused demo index for fresh-clone validation:
- onboarding
- gateway-auth
- gateway-remote-access
- multi-channel-live (Telegram/Discord/WhatsApp)
- deployment-wasm

Options:
  --repo-root PATH   Repository root (defaults to caller-derived root)
  --binary PATH      tau-coding-agent binary path (default: <repo-root>/target/debug/tau-coding-agent)
  --skip-build       Skip cargo build and require --binary to exist
  --list             Print selected scenarios and exit without execution
  --only SCENARIOS   Comma-separated subset (names: onboarding,gateway-auth,gateway-remote-access,multi-channel-live,deployment-wasm)
  --json             Emit deterministic JSON output for list/summary modes
  --report-file      Write deterministic JSON report artifact to path
  --fail-fast        Stop after first failed scenario
  --timeout-seconds  Positive integer timeout for each scenario wrapper
  --help             Show this usage message
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
      IFS=',' read -r -a requested_scenario_names <<< "$2"
      for requested_scenario_name in "${requested_scenario_names[@]}"; do
        trimmed_requested_scenario_name="$(trim_spaces "${requested_scenario_name}")"
        if [[ -z "${trimmed_requested_scenario_name}" ]]; then
          continue
        fi
        if normalized_scenario_name="$(normalize_scenario_name "${trimmed_requested_scenario_name}")"; then
          selected_scenario_lookup["${normalized_scenario_name}"]=1
        else
          unknown_scenario_names+=("${trimmed_requested_scenario_name}")
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

if [[ ${#unknown_scenario_names[@]} -gt 0 ]]; then
  log_error "unknown scenario names in --only: ${unknown_scenario_names[*]}"
  exit 2
fi

selected_scenarios=("${scenario_ids[@]}")
if [[ "${has_only_filter}" == "true" ]]; then
  selected_scenarios=()
  for scenario_id in "${scenario_ids[@]}"; do
    if [[ -n "${selected_scenario_lookup[${scenario_id}]:-}" ]]; then
      selected_scenarios+=("${scenario_id}")
    fi
  done
  if [[ ${#selected_scenarios[@]} -eq 0 ]]; then
    log_error "no scenarios selected by --only filter"
    exit 2
  fi
fi

if [[ "${list_only}" == "true" ]]; then
  list_json_payload="$(print_scenario_list_json "${selected_scenarios[@]}")"
  if [[ -n "${report_file}" ]]; then
    write_report_file "${list_json_payload}" "${report_file}"
  fi
  if [[ "${json_output}" == "true" ]]; then
    echo "${list_json_payload}"
  else
    print_scenario_list_text "${selected_scenarios[@]}"
  fi
  exit 0
fi

prepare_binary_once || exit $?

total=0
passed=0
failed=0

for scenario_id in "${selected_scenarios[@]}"; do
  total=$((total + 1))
  wrapper_script="$(scenario_wrapper "${scenario_id}")"
  log_info "[demo:index] [${total}] ${scenario_id} (${wrapper_script})"
  args=("${script_dir}/${wrapper_script}" "--repo-root" "${repo_root}" "--binary" "${binary}" "--skip-build")
  if [[ -n "${timeout_seconds}" ]]; then
    args+=("--timeout-seconds" "${timeout_seconds}")
  fi

  started_ms="$(current_time_ms)"
  if [[ "${json_output}" == "true" ]]; then
    if "${args[@]}" >&2; then
      scenario_exit_code=0
      scenario_status="passed"
    else
      scenario_exit_code=$?
      scenario_status="failed"
    fi
  elif "${args[@]}"; then
    scenario_exit_code=0
    scenario_status="passed"
  else
    scenario_exit_code=$?
    scenario_status="failed"
  fi
  ended_ms="$(current_time_ms)"
  scenario_duration_ms=$((ended_ms - started_ms))
  if [[ "${scenario_duration_ms}" -lt 0 ]]; then
    scenario_duration_ms=0
  fi

  run_scenario_names+=("${scenario_id}")
  run_scenario_statuses+=("${scenario_status}")
  run_scenario_exit_codes+=("${scenario_exit_code}")
  run_scenario_durations_ms+=("${scenario_duration_ms}")

  if [[ "${scenario_status}" == "passed" ]]; then
    passed=$((passed + 1))
    log_info "[demo:index] PASS ${scenario_id}"
  else
    failed=$((failed + 1))
    hint="$(scenario_troubleshooting_hint "${scenario_id}")"
    log_error "[demo:index] FAIL ${scenario_id} exit=${scenario_exit_code}"
    log_error "[demo:index] remediation: ${hint}"
    if [[ "${fail_fast}" == "true" ]]; then
      log_error "[demo:index] fail-fast triggered; stopping after ${scenario_id}"
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
  echo "[demo:index] summary: total=${total} passed=${passed} failed=${failed}"
fi

if [[ ${failed} -gt 0 ]]; then
  exit 1
fi
