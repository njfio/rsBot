#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
MATRIX_SCRIPT="${SCRIPT_DIR}/live-capability-matrix.sh"
MATRIX_TEST_SCRIPT="${SCRIPT_DIR}/test-live-capability-matrix.sh"

KEY_FILE="${TAU_PROVIDER_KEYS_FILE:-${REPO_ROOT}/.tau/provider-keys.env}"
OUTPUT_ROOT="${TAU_LIVE_CAPABILITY_OUTPUT_ROOT:-${REPO_ROOT}/.tau/reports/live-validation}"
RUN_ID="${TAU_ADVANCED_VALIDATION_RUN_ID:-$(date -u +"%Y%m%d-%H%M%S")-advanced}"
TAU_BIN="${TAU_LIVE_CAPABILITY_BIN:-${REPO_ROOT}/target/debug/tau-coding-agent}"
TIMEOUT_MS="${TAU_LIVE_CAPABILITY_TIMEOUT_MS:-180000}"
MAX_TURNS="${TAU_LIVE_CAPABILITY_MAX_TURNS:-12}"
LONG_OUTPUT_MIN_WORDS="${TAU_LIVE_CAPABILITY_LONG_OUTPUT_MIN_WORDS:-900}"
CASES_CSV="${TAU_ADVANCED_VALIDATION_CASES:-research_openai_codex,research_openrouter_kimi,blog_openrouter_minimax,long_output_openai_codex,stream_openai_research,stream_anthropic_blog,stream_google_snake,stream_openrouter_xai_blog,session_continuity_openai,parallel_tools_openai}"
SKIP_BUILD="false"

usage() {
  cat <<'EOF'
Usage: validate-advanced-capabilities.sh [options]

Run advanced validation coverage for capability items 1-7:
  1) OpenAI Codex direct E2E
  2) OpenRouter Kimi + Minimax
  3) Long-output stress
  4) Streaming mode across providers
  5) Retry/failure path tests (timeout, 429, backoff, retry budget)
  6) Session continuity stop/resume
  7) Multi-tool execution behavior

Options:
  --key-file <path>        Provider key env file (default: .tau/provider-keys.env)
  --output-root <path>     Output report root (default: .tau/reports/live-validation)
  --run-id <id>            Run identifier (default: UTC timestamp with -advanced suffix)
  --bin <path>             tau-coding-agent binary path
  --timeout-ms <ms>        Provider timeout per request (default: 180000)
  --max-turns <n>          Max turns per live case (default: 12)
  --long-output-min-words  Minimum words for long-output case (default: 900)
  --cases <csv>            Override advanced case list
  --skip-build             Do not build binary if missing
  --help                   Show this help text
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --key-file)
      KEY_FILE="$2"
      shift 2
      ;;
    --output-root)
      OUTPUT_ROOT="$2"
      shift 2
      ;;
    --run-id)
      RUN_ID="$2"
      shift 2
      ;;
    --bin)
      TAU_BIN="$2"
      shift 2
      ;;
    --timeout-ms)
      TIMEOUT_MS="$2"
      shift 2
      ;;
    --max-turns)
      MAX_TURNS="$2"
      shift 2
      ;;
    --long-output-min-words)
      LONG_OUTPUT_MIN_WORDS="$2"
      shift 2
      ;;
    --cases)
      CASES_CSV="$2"
      shift 2
      ;;
    --skip-build)
      SKIP_BUILD="true"
      shift
      ;;
    --help|-h)
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

echo "[1/3] deterministic harness checks"
bash "$MATRIX_TEST_SCRIPT"

echo "[2/3] retry/failure-path regression checks"
cargo test -p tau-ai --test provider_http_integration openai_client_retries_on_rate_limit_then_succeeds -- --exact --test-threads=1
cargo test -p tau-ai --test provider_http_integration integration_openai_client_respects_retry_after_header_floor -- --exact --test-threads=1
cargo test -p tau-ai --test provider_http_integration regression_openai_client_returns_timeout_error_when_server_is_slow -- --exact --test-threads=1
cargo test -p tau-ai --test provider_http_integration openai_client_retry_budget_can_block_retries -- --exact --test-threads=1

echo "[3/3] live provider matrix"
matrix_args=(
  --key-file "$KEY_FILE"
  --output-root "$OUTPUT_ROOT"
  --run-id "$RUN_ID"
  --timeout-ms "$TIMEOUT_MS"
  --max-turns "$MAX_TURNS"
  --long-output-min-words "$LONG_OUTPUT_MIN_WORDS"
  --cases "$CASES_CSV"
  --bin "$TAU_BIN"
)
if [[ "$SKIP_BUILD" == "true" ]]; then
  matrix_args+=(--skip-build)
fi
"$MATRIX_SCRIPT" "${matrix_args[@]}"

SUMMARY_PATH="${OUTPUT_ROOT}/${RUN_ID}-capability-matrix/summary.tsv"
if [[ ! -f "$SUMMARY_PATH" ]]; then
  echo "error: summary file not found: $SUMMARY_PATH" >&2
  exit 1
fi

FAILED_ROWS="$(awk -F '\t' 'NR > 1 && !($5 == "PASS" && $7 == "PASS" && $8 == "PASS") { print $0 }' "$SUMMARY_PATH")"
if [[ -n "$FAILED_ROWS" ]]; then
  echo "error: one or more advanced live cases failed:" >&2
  echo "$FAILED_ROWS" >&2
  exit 1
fi

echo ""
echo "advanced capability validation: PASS"
echo "summary: $SUMMARY_PATH"
