#!/usr/bin/env bash
set -euo pipefail

KEY_FILE="${TAU_PROVIDER_KEYS_FILE:-.tau/provider-keys.env}"
TIMEOUT_MS="${TAU_PROVIDER_SMOKE_TIMEOUT_MS:-30000}"
PROMPT_TEXT="${TAU_PROVIDER_SMOKE_PROMPT:-Reply with the single token OK.}"

if [[ ! -f "$KEY_FILE" ]]; then
  cat <<MSG
provider-live-smoke: key file not found: $KEY_FILE
Create it from template:
  cp scripts/dev/provider-keys.env.example .tau/provider-keys.env
  chmod 600 .tau/provider-keys.env
MSG
  exit 1
fi

set -a
# shellcheck disable=SC1090
source "$KEY_FILE"
set +a

mkdir -p .tau/reports
SMOKE_LOG_DIR=".tau/reports/provider-smoke"
mkdir -p "$SMOKE_LOG_DIR"

if [[ -n "${TAU_PROVIDER_SMOKE_BIN:-}" ]]; then
  TAU_BIN="$TAU_PROVIDER_SMOKE_BIN"
else
  echo "[build] compiling tau-coding-agent binary"
  cargo build -p tau-coding-agent --quiet
  TAU_BIN="target/debug/tau-coding-agent"
fi

if [[ ! -x "$TAU_BIN" ]]; then
  echo "provider-live-smoke: executable not found: $TAU_BIN"
  exit 1
fi

SUCCESS_COUNT=0
SKIP_COUNT=0
FAIL_COUNT=0

run_case() {
  local case_name="$1"
  local key_value="$2"
  shift 2

  if [[ -z "$key_value" ]]; then
    echo "[skip] $case_name (missing key)"
    SKIP_COUNT=$((SKIP_COUNT + 1))
    return 0
  fi

  local log_path="$SMOKE_LOG_DIR/${case_name}.log"
  echo "[run] $case_name"
  if TAU_ONBOARD_AUTO=false "$TAU_BIN" \
    --request-timeout-ms "$TIMEOUT_MS" \
    --provider-subscription-strict=true \
    --prompt "$PROMPT_TEXT" \
    "$@" >"$log_path" 2>&1; then
    echo "[ok]  $case_name"
    SUCCESS_COUNT=$((SUCCESS_COUNT + 1))
  else
    echo "[fail] $case_name (see $log_path)"
    tail -n 30 "$log_path" || true
    FAIL_COUNT=$((FAIL_COUNT + 1))
  fi
}

OPENAI_KEY="${OPENAI_API_KEY:-${TAU_API_KEY:-}}"
OPENROUTER_KEY="${OPENROUTER_API_KEY:-${TAU_OPENROUTER_API_KEY:-}}"
DEEPSEEK_KEY="${DEEPSEEK_API_KEY:-${TAU_DEEPSEEK_API_KEY:-}}"
XAI_KEY="${XAI_API_KEY:-}"
MISTRAL_KEY="${MISTRAL_API_KEY:-}"
GROQ_KEY="${GROQ_API_KEY:-}"
ANTHROPIC_KEY="${ANTHROPIC_API_KEY:-}"
GOOGLE_KEY="${GEMINI_API_KEY:-${GOOGLE_API_KEY:-}}"

run_case "openai" "$OPENAI_KEY" \
  --model "${TAU_OPENAI_MODEL:-openai/gpt-4o-mini}" \
  --api-base "${TAU_OPENAI_API_BASE:-https://api.openai.com/v1}" \
  --openai-api-key "$OPENAI_KEY" \
  --openai-auth-mode api-key

run_case "openrouter" "$OPENROUTER_KEY" \
  --model "${TAU_OPENROUTER_MODEL:-openrouter/openai/gpt-4.1-mini}" \
  --api-base "${TAU_OPENROUTER_API_BASE:-https://openrouter.ai/api/v1}" \
  --openai-api-key "$OPENROUTER_KEY" \
  --openai-auth-mode api-key

run_case "deepseek" "$DEEPSEEK_KEY" \
  --model "${TAU_DEEPSEEK_MODEL:-deepseek/deepseek-chat}" \
  --api-base "${TAU_DEEPSEEK_API_BASE:-https://api.deepseek.com}" \
  --openai-api-key "$DEEPSEEK_KEY" \
  --openai-auth-mode api-key

run_case "xai" "$XAI_KEY" \
  --model "${TAU_XAI_MODEL:-xai/grok-4}" \
  --api-base "${TAU_XAI_API_BASE:-https://api.x.ai/v1}" \
  --openai-api-key "$XAI_KEY" \
  --openai-auth-mode api-key

run_case "mistral" "$MISTRAL_KEY" \
  --model "${TAU_MISTRAL_MODEL:-mistral/mistral-large-3}" \
  --api-base "${TAU_MISTRAL_API_BASE:-https://api.mistral.ai/v1}" \
  --openai-api-key "$MISTRAL_KEY" \
  --openai-auth-mode api-key

run_case "groq" "$GROQ_KEY" \
  --model "${TAU_GROQ_MODEL:-groq/llama-3.3-70b}" \
  --api-base "${TAU_GROQ_API_BASE:-https://api.groq.com/openai/v1}" \
  --openai-api-key "$GROQ_KEY" \
  --openai-auth-mode api-key

run_case "anthropic" "$ANTHROPIC_KEY" \
  --model "${TAU_ANTHROPIC_MODEL:-anthropic/claude-sonnet-4-20250514}" \
  --anthropic-api-key "$ANTHROPIC_KEY" \
  --anthropic-auth-mode api-key

run_case "google" "$GOOGLE_KEY" \
  --model "${TAU_GOOGLE_MODEL:-google/gemini-2.5-pro}" \
  --google-api-key "$GOOGLE_KEY" \
  --google-auth-mode api-key

echo ""
echo "provider-live-smoke summary: ok=$SUCCESS_COUNT skipped=$SKIP_COUNT failed=$FAIL_COUNT"
if [[ "$FAIL_COUNT" -gt 0 ]]; then
  exit 1
fi
