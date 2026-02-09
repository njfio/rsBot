#!/usr/bin/env bash
set -euo pipefail

tau_demo_common_print_usage() {
  local script_name="$1"
  local summary="$2"
  cat <<EOF
Usage: ${script_name} [--repo-root PATH] [--binary PATH] [--skip-build] [--timeout-seconds N] [--help]

${summary}

Options:
  --repo-root PATH  Repository root (defaults to caller-derived root)
  --binary PATH     tau-coding-agent binary path (default: <repo-root>/target/debug/tau-coding-agent)
  --skip-build      Skip cargo build and require --binary to exist
  --timeout-seconds Positive integer timeout for each demo step
  --help            Show this usage message
EOF
}

tau_demo_common_init() {
  local demo_name="$1"
  local summary="$2"
  shift 2

  local caller_script="${BASH_SOURCE[1]}"
  local caller_dir
  caller_dir="$(cd "$(dirname "${caller_script}")" && pwd)"

  TAU_DEMO_NAME="${demo_name}"
  TAU_DEMO_SUMMARY="${summary}"
  TAU_DEMO_REPO_ROOT="$(cd "${caller_dir}/../.." && pwd)"
  TAU_DEMO_BINARY="${TAU_DEMO_REPO_ROOT}/target/debug/tau-coding-agent"
  TAU_DEMO_SKIP_BUILD="false"
  TAU_DEMO_TIMEOUT_SECONDS=""
  TAU_DEMO_STEP_TOTAL=0
  TAU_DEMO_STEP_PASSED=0

  while [[ $# -gt 0 ]]; do
    case "$1" in
      --repo-root)
        if [[ $# -lt 2 ]]; then
          echo "missing value for --repo-root" >&2
          tau_demo_common_print_usage "$(basename "${caller_script}")" "${summary}" >&2
          return 2
        fi
        TAU_DEMO_REPO_ROOT="$2"
        shift 2
        ;;
      --binary)
        if [[ $# -lt 2 ]]; then
          echo "missing value for --binary" >&2
          tau_demo_common_print_usage "$(basename "${caller_script}")" "${summary}" >&2
          return 2
        fi
        TAU_DEMO_BINARY="$2"
        shift 2
        ;;
      --skip-build)
        TAU_DEMO_SKIP_BUILD="true"
        shift
        ;;
      --timeout-seconds)
        if [[ $# -lt 2 ]]; then
          echo "missing value for --timeout-seconds" >&2
          tau_demo_common_print_usage "$(basename "${caller_script}")" "${summary}" >&2
          return 2
        fi
        if [[ ! "$2" =~ ^[1-9][0-9]*$ ]]; then
          echo "invalid value for --timeout-seconds (expected positive integer): $2" >&2
          tau_demo_common_print_usage "$(basename "${caller_script}")" "${summary}" >&2
          return 2
        fi
        TAU_DEMO_TIMEOUT_SECONDS="$2"
        shift 2
        ;;
      --help)
        tau_demo_common_print_usage "$(basename "${caller_script}")" "${summary}"
        return 64
        ;;
      *)
        echo "unknown argument: $1" >&2
        tau_demo_common_print_usage "$(basename "${caller_script}")" "${summary}" >&2
        return 2
        ;;
    esac
  done

  if [[ ! -d "${TAU_DEMO_REPO_ROOT}" ]]; then
    echo "invalid --repo-root path (directory not found): ${TAU_DEMO_REPO_ROOT}" >&2
    return 2
  fi
  TAU_DEMO_REPO_ROOT="$(cd "${TAU_DEMO_REPO_ROOT}" && pwd)"

  if [[ "${TAU_DEMO_BINARY}" != /* ]]; then
    TAU_DEMO_BINARY="${TAU_DEMO_REPO_ROOT}/${TAU_DEMO_BINARY}"
  fi

  if [[ -n "${TAU_DEMO_TIMEOUT_SECONDS}" ]]; then
    tau_demo_common_require_command python3 || return 1
  fi
}

tau_demo_common_require_command() {
  local executable="$1"
  if ! command -v "${executable}" >/dev/null 2>&1; then
    echo "missing required executable: ${executable}" >&2
    return 1
  fi
}

tau_demo_common_require_file() {
  local path="$1"
  if [[ ! -f "${path}" ]]; then
    echo "missing required file: ${path}" >&2
    return 1
  fi
}

tau_demo_common_require_dir() {
  local path="$1"
  if [[ ! -d "${path}" ]]; then
    echo "missing required directory: ${path}" >&2
    return 1
  fi
}

tau_demo_common_prepare_binary() {
  if [[ "${TAU_DEMO_SKIP_BUILD}" == "true" ]]; then
    if [[ ! -f "${TAU_DEMO_BINARY}" ]]; then
      echo "missing tau-coding-agent binary (use --binary or remove --skip-build): ${TAU_DEMO_BINARY}" >&2
      return 1
    fi
    return 0
  fi

  tau_demo_common_require_command cargo
  echo "[demo:${TAU_DEMO_NAME}] building tau-coding-agent"
  (
    cd "${TAU_DEMO_REPO_ROOT}"
    cargo build -p tau-coding-agent >/dev/null
  )
  tau_demo_common_require_file "${TAU_DEMO_BINARY}"
}

tau_demo_common_exec_binary() {
  if [[ -z "${TAU_DEMO_TIMEOUT_SECONDS}" ]]; then
    (
      cd "${TAU_DEMO_REPO_ROOT}"
      "${TAU_DEMO_BINARY}" "$@"
    )
    return $?
  fi

  (
    cd "${TAU_DEMO_REPO_ROOT}"
    python3 - "${TAU_DEMO_TIMEOUT_SECONDS}" "${TAU_DEMO_BINARY}" "$@" <<'PY'
import subprocess
import sys

timeout_seconds = int(sys.argv[1])
binary = sys.argv[2]
args = sys.argv[3:]

try:
    completed = subprocess.run([binary, *args], timeout=timeout_seconds)
except subprocess.TimeoutExpired:
    sys.exit(124)

sys.exit(completed.returncode)
PY
  )
  return $?
}

tau_demo_common_run_step() {
  local label="$1"
  shift

  TAU_DEMO_STEP_TOTAL=$((TAU_DEMO_STEP_TOTAL + 1))
  local rendered_command
  printf -v rendered_command '%q ' "${TAU_DEMO_BINARY}" "$@"
  rendered_command="${rendered_command% }"

  echo "[demo:${TAU_DEMO_NAME}] [${TAU_DEMO_STEP_TOTAL}] ${label}"
  echo "[demo:${TAU_DEMO_NAME}] command: ${rendered_command}"

  if [[ -n "${TAU_DEMO_TRACE_LOG:-}" ]]; then
    printf '%s\t%s\n' "${label}" "${rendered_command}" >>"${TAU_DEMO_TRACE_LOG}"
  fi

  if tau_demo_common_exec_binary "$@"; then
    TAU_DEMO_STEP_PASSED=$((TAU_DEMO_STEP_PASSED + 1))
    echo "[demo:${TAU_DEMO_NAME}] PASS ${label}"
    return 0
  else
    local rc=$?
    if [[ "${rc}" -eq 124 && -n "${TAU_DEMO_TIMEOUT_SECONDS}" ]]; then
      echo "[demo:${TAU_DEMO_NAME}] TIMEOUT ${label} after ${TAU_DEMO_TIMEOUT_SECONDS}s" >&2
    else
      echo "[demo:${TAU_DEMO_NAME}] FAIL ${label} exit=${rc}" >&2
    fi
    return "${rc}"
  fi
}

tau_demo_common_finish() {
  local failed=$((TAU_DEMO_STEP_TOTAL - TAU_DEMO_STEP_PASSED))
  echo "[demo:${TAU_DEMO_NAME}] summary: total=${TAU_DEMO_STEP_TOTAL} passed=${TAU_DEMO_STEP_PASSED} failed=${failed}"
}
