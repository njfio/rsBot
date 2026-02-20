#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${TAU_CORTEX_BASE_URL:-http://127.0.0.1:8787}"
AUTH_MODE="${TAU_CORTEX_AUTH_MODE:-token}"
AUTH_TOKEN="${TAU_CORTEX_AUTH_TOKEN:-}"
PROBE_INPUT="${TAU_CORTEX_PROBE_INPUT:-cortex readiness probe}"
EXPECT_HEALTH_STATE="${TAU_CORTEX_EXPECT_HEALTH_STATE:-healthy}"
TIMEOUT_SECONDS="${TAU_CORTEX_TIMEOUT_SECONDS:-20}"
QUIET_MODE="false"

usage() {
  cat <<'USAGE'
Usage: cortex-readiness-live-check.sh [options]

Runs authenticated cortex readiness live validation:
1) probes POST /cortex/chat SSE contract
2) validates GET /cortex/status readiness fields

Options:
  --base-url <url>             Gateway base URL (default: http://127.0.0.1:8787)
  --auth-mode <token|none>     Auth mode (default: token)
  --auth-token <token>         Bearer token (or use TAU_CORTEX_AUTH_TOKEN)
  --probe-input <text>         Probe input payload for /cortex/chat
  --expect-health-state <v>    Expected cortex status health_state (default: healthy)
  --timeout-seconds <n>        Curl timeout seconds (default: 20)
  --quiet                      Suppress informational logs
  --help                       Show this help text
USAGE
}

log_info() {
  if [[ "${QUIET_MODE}" != "true" ]]; then
    echo "$@"
  fi
}

require_cmd() {
  local name="$1"
  if ! command -v "${name}" >/dev/null 2>&1; then
    echo "error: required command '${name}' not found" >&2
    exit 1
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --base-url)
      BASE_URL="$2"
      shift 2
      ;;
    --auth-mode)
      AUTH_MODE="$2"
      shift 2
      ;;
    --auth-token)
      AUTH_TOKEN="$2"
      shift 2
      ;;
    --probe-input)
      PROBE_INPUT="$2"
      shift 2
      ;;
    --expect-health-state)
      EXPECT_HEALTH_STATE="$2"
      shift 2
      ;;
    --timeout-seconds)
      TIMEOUT_SECONDS="$2"
      shift 2
      ;;
    --quiet)
      QUIET_MODE="true"
      shift
      ;;
    --help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown option '$1'" >&2
      usage >&2
      exit 1
      ;;
  esac
done

require_cmd curl
require_cmd jq
require_cmd rg

if ! [[ "${TIMEOUT_SECONDS}" =~ ^[0-9]+$ ]]; then
  echo "error: --timeout-seconds must be a non-negative integer" >&2
  exit 1
fi

if [[ "${AUTH_MODE}" != "token" ]] && [[ "${AUTH_MODE}" != "none" ]]; then
  echo "error: --auth-mode must be token or none" >&2
  exit 1
fi

if [[ "${AUTH_MODE}" == "token" ]] && [[ -z "${AUTH_TOKEN}" ]]; then
  echo "error: auth token required for token mode (set --auth-token or TAU_CORTEX_AUTH_TOKEN)" >&2
  exit 1
fi

auth_args=()
if [[ "${AUTH_MODE}" == "token" ]]; then
  auth_args=("-H" "Authorization: Bearer ${AUTH_TOKEN}")
fi

chat_url="${BASE_URL%/}/cortex/chat"
status_url="${BASE_URL%/}/cortex/status"
chat_payload="$(jq -cn --arg input "${PROBE_INPUT}" '{input: $input}')"

log_info "cortex-readiness-live-check"
log_info "base_url=${BASE_URL}"
log_info "auth_mode=${AUTH_MODE}"

chat_response="$(curl -sS --fail-with-body --max-time "${TIMEOUT_SECONDS}" \
  "${auth_args[@]}" \
  -H "Content-Type: application/json" \
  -H "Accept: text/event-stream" \
  -d "${chat_payload}" \
  "${chat_url}")"

if ! printf '%s\n' "${chat_response}" | rg -q 'event:\s*cortex\.response\.created'; then
  echo "error: cortex chat SSE missing cortex.response.created event" >&2
  exit 1
fi

if ! printf '%s\n' "${chat_response}" | rg -q 'event:\s*cortex\.response\.output_text\.done'; then
  echo "error: cortex chat SSE missing cortex.response.output_text.done event" >&2
  exit 1
fi

status_payload="$(curl -sS --fail-with-body --max-time "${TIMEOUT_SECONDS}" \
  "${auth_args[@]}" \
  "${status_url}")"

health_state="$(printf '%s' "${status_payload}" | jq -r '.health_state // ""')"
rollout_gate="$(printf '%s' "${status_payload}" | jq -r '.rollout_gate // ""')"
reason_code="$(printf '%s' "${status_payload}" | jq -r '.reason_code // ""')"
chat_count="$(printf '%s' "${status_payload}" | jq -r '.event_type_counts["cortex.chat.request"] // 0')"

if [[ -z "${reason_code}" ]]; then
  echo "error: cortex status response is missing reason_code" >&2
  printf '%s\n' "${status_payload}" >&2
  exit 1
fi

if [[ "${health_state}" != "${EXPECT_HEALTH_STATE}" ]]; then
  echo "error: expected health_state='${EXPECT_HEALTH_STATE}' but got '${health_state}'" >&2
  printf '%s\n' "${status_payload}" >&2
  exit 1
fi

if [[ "${EXPECT_HEALTH_STATE}" == "healthy" ]]; then
  if [[ "${rollout_gate}" != "pass" ]]; then
    echo "error: expected rollout_gate='pass' when healthy but got '${rollout_gate}'" >&2
    printf '%s\n' "${status_payload}" >&2
    exit 1
  fi
  if ! [[ "${chat_count}" =~ ^[0-9]+$ ]] || (( chat_count < 1 )); then
    echo "error: expected cortex.chat.request event count >= 1 but got '${chat_count}'" >&2
    printf '%s\n' "${status_payload}" >&2
    exit 1
  fi
fi

log_info "health_state=${health_state}"
log_info "rollout_gate=${rollout_gate}"
log_info "reason_code=${reason_code}"
log_info "cortex_chat_event_count=${chat_count}"
log_info "status=pass"
