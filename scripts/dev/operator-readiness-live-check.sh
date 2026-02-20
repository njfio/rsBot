#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${TAU_OPERATOR_BASE_URL:-http://127.0.0.1:8787}"
AUTH_MODE="${TAU_OPERATOR_AUTH_MODE:-token}"
AUTH_TOKEN="${TAU_OPERATOR_AUTH_TOKEN:-}"
EXPECT_GATEWAY_HEALTH_STATE="${TAU_OPERATOR_EXPECT_GATEWAY_HEALTH_STATE:-healthy}"
EXPECT_CORTEX_HEALTH_STATE="${TAU_OPERATOR_EXPECT_CORTEX_HEALTH_STATE:-healthy}"
EXPECT_OPERATOR_HEALTH_STATE="${TAU_OPERATOR_EXPECT_OPERATOR_HEALTH_STATE:-healthy}"
EXPECT_GATEWAY_ROLLOUT_GATE="${TAU_OPERATOR_EXPECT_GATEWAY_ROLLOUT_GATE:-pass}"
EXPECT_CORTEX_ROLLOUT_GATE="${TAU_OPERATOR_EXPECT_CORTEX_ROLLOUT_GATE:-pass}"
EXPECT_OPERATOR_ROLLOUT_GATE="${TAU_OPERATOR_EXPECT_OPERATOR_ROLLOUT_GATE:-pass}"
TIMEOUT_SECONDS="${TAU_OPERATOR_TIMEOUT_SECONDS:-20}"
QUIET_MODE="false"

usage() {
  cat <<'USAGE'
Usage: operator-readiness-live-check.sh [options]

Run fail-closed operator readiness validation:
1) GET /gateway/status
2) GET /cortex/status
3) cargo run -- --operator-control-summary --operator-control-summary-json

Options:
  --base-url <url>                     Gateway base URL (default: http://127.0.0.1:8787)
  --auth-mode <token|none>             Auth mode for gateway/cortex endpoints (default: token)
  --auth-token <token>                 Bearer token (or TAU_OPERATOR_AUTH_TOKEN)
  --expect-gateway-health-state <v>    Expected gateway health_state (default: healthy)
  --expect-cortex-health-state <v>     Expected cortex health_state (default: healthy)
  --expect-operator-health-state <v>   Expected operator summary health_state (default: healthy)
  --expect-gateway-rollout-gate <v>    Expected gateway rollout_gate (default: pass)
  --expect-cortex-rollout-gate <v>     Expected cortex rollout_gate (default: pass)
  --expect-operator-rollout-gate <v>   Expected operator summary rollout_gate (default: pass)
  --expect-rollout-gate <v>            Alias: sets all three rollout expectations
  --timeout-seconds <n>                Curl timeout seconds (default: 20)
  --quiet                              Suppress informational logs
  --help                               Show this help text
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
    --expect-gateway-health-state)
      EXPECT_GATEWAY_HEALTH_STATE="$2"
      shift 2
      ;;
    --expect-cortex-health-state)
      EXPECT_CORTEX_HEALTH_STATE="$2"
      shift 2
      ;;
    --expect-operator-health-state)
      EXPECT_OPERATOR_HEALTH_STATE="$2"
      shift 2
      ;;
    --expect-gateway-rollout-gate)
      EXPECT_GATEWAY_ROLLOUT_GATE="$2"
      shift 2
      ;;
    --expect-cortex-rollout-gate)
      EXPECT_CORTEX_ROLLOUT_GATE="$2"
      shift 2
      ;;
    --expect-operator-rollout-gate)
      EXPECT_OPERATOR_ROLLOUT_GATE="$2"
      shift 2
      ;;
    --expect-rollout-gate)
      EXPECT_GATEWAY_ROLLOUT_GATE="$2"
      EXPECT_CORTEX_ROLLOUT_GATE="$2"
      EXPECT_OPERATOR_ROLLOUT_GATE="$2"
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
require_cmd cargo

if ! [[ "${TIMEOUT_SECONDS}" =~ ^[0-9]+$ ]]; then
  echo "error: --timeout-seconds must be a non-negative integer" >&2
  exit 1
fi

if [[ "${AUTH_MODE}" != "token" ]] && [[ "${AUTH_MODE}" != "none" ]]; then
  echo "error: --auth-mode must be token or none" >&2
  exit 1
fi

if [[ "${AUTH_MODE}" == "token" ]] && [[ -z "${AUTH_TOKEN}" ]]; then
  echo "error: auth token required for token mode (set --auth-token or TAU_OPERATOR_AUTH_TOKEN)" >&2
  exit 1
fi

auth_args=()
if [[ "${AUTH_MODE}" == "token" ]]; then
  auth_args=("-H" "Authorization: Bearer ${AUTH_TOKEN}")
fi

gateway_url="${BASE_URL%/}/gateway/status"
cortex_url="${BASE_URL%/}/cortex/status"

log_info "operator-readiness-live-check"
log_info "base_url=${BASE_URL}"
log_info "auth_mode=${AUTH_MODE}"

gateway_payload="$(curl -sS --fail-with-body --max-time "${TIMEOUT_SECONDS}" \
  "${auth_args[@]}" \
  "${gateway_url}")"

gateway_health="$(printf '%s' "${gateway_payload}" | jq -r '.health_state // .events.health_state // (if (.service.service_status // "") == "running" then "healthy" else "" end)')"
gateway_rollout="$(printf '%s' "${gateway_payload}" | jq -r '.rollout_gate // .service.rollout_gate // .events.rollout_gate // ""')"
gateway_reason="$(printf '%s' "${gateway_payload}" | jq -r '.rollout_reason_code // .reason_code // .service.rollout_reason_code // .service.guardrail_reason_code // .events.reason_code // .events.rollout_reason_code // ""')"

if [[ -z "${gateway_reason}" ]]; then
  echo "error: gateway status response missing rollout reason code" >&2
  printf '%s\n' "${gateway_payload}" >&2
  exit 1
fi

if [[ "${gateway_health}" != "${EXPECT_GATEWAY_HEALTH_STATE}" ]]; then
  echo "error: gateway status health_state expected '${EXPECT_GATEWAY_HEALTH_STATE}' but got '${gateway_health}'" >&2
  printf '%s\n' "${gateway_payload}" >&2
  exit 1
fi

if [[ "${gateway_rollout}" != "${EXPECT_GATEWAY_ROLLOUT_GATE}" ]]; then
  echo "error: gateway status rollout_gate='${gateway_rollout}' expected '${EXPECT_GATEWAY_ROLLOUT_GATE}' (reason='${gateway_reason}')" >&2
  printf '%s\n' "${gateway_payload}" >&2
  exit 1
fi

cortex_payload="$(curl -sS --fail-with-body --max-time "${TIMEOUT_SECONDS}" \
  "${auth_args[@]}" \
  "${cortex_url}")"

cortex_health="$(printf '%s' "${cortex_payload}" | jq -r '.health_state // ""')"
cortex_rollout="$(printf '%s' "${cortex_payload}" | jq -r '.rollout_gate // ""')"
cortex_reason="$(printf '%s' "${cortex_payload}" | jq -r '.reason_code // ""')"

if [[ -z "${cortex_reason}" ]]; then
  echo "error: cortex status response missing reason_code" >&2
  printf '%s\n' "${cortex_payload}" >&2
  exit 1
fi

if [[ "${cortex_health}" != "${EXPECT_CORTEX_HEALTH_STATE}" ]]; then
  echo "error: cortex status health_state expected '${EXPECT_CORTEX_HEALTH_STATE}' but got '${cortex_health}'" >&2
  printf '%s\n' "${cortex_payload}" >&2
  exit 1
fi

if [[ "${cortex_rollout}" != "${EXPECT_CORTEX_ROLLOUT_GATE}" ]]; then
  echo "error: cortex status rollout_gate='${cortex_rollout}' expected '${EXPECT_CORTEX_ROLLOUT_GATE}' (reason='${cortex_reason}')" >&2
  printf '%s\n' "${cortex_payload}" >&2
  exit 1
fi

operator_payload="$(cargo run -p tau-coding-agent -- \
  --operator-control-summary \
  --operator-control-summary-json)"

operator_health="$(printf '%s' "${operator_payload}" | jq -r '.health_state // ""')"
operator_rollout="$(printf '%s' "${operator_payload}" | jq -r '.rollout_gate // ""')"
operator_reason_codes="$(printf '%s' "${operator_payload}" | jq -r '(.reason_codes // []) | join(",")')"

if [[ -z "${operator_reason_codes}" ]]; then
  echo "error: operator control summary response missing reason_codes" >&2
  printf '%s\n' "${operator_payload}" >&2
  exit 1
fi

if [[ "${operator_health}" != "${EXPECT_OPERATOR_HEALTH_STATE}" ]]; then
  echo "error: operator control summary health_state expected '${EXPECT_OPERATOR_HEALTH_STATE}' but got '${operator_health}'" >&2
  printf '%s\n' "${operator_payload}" >&2
  exit 1
fi

if [[ "${operator_rollout}" != "${EXPECT_OPERATOR_ROLLOUT_GATE}" ]]; then
  echo "error: operator control summary rollout_gate='${operator_rollout}' expected '${EXPECT_OPERATOR_ROLLOUT_GATE}' (reason_codes='${operator_reason_codes}')" >&2
  printf '%s\n' "${operator_payload}" >&2
  exit 1
fi

log_info "gateway_health_state=${gateway_health}"
log_info "gateway_rollout_gate=${gateway_rollout}"
log_info "gateway_reason=${gateway_reason}"
log_info "cortex_health_state=${cortex_health}"
log_info "cortex_rollout_gate=${cortex_rollout}"
log_info "cortex_reason_code=${cortex_reason}"
log_info "operator_health_state=${operator_health}"
log_info "operator_rollout_gate=${operator_rollout}"
log_info "operator_reason_codes=${operator_reason_codes}"
log_info "status=pass"
