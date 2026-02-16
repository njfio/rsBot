#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

KEY_FILE="${TAU_PROVIDER_KEYS_FILE:-${REPO_ROOT}/.tau/provider-keys.env}"
OUTPUT_ROOT="${TAU_LIVE_CAPABILITY_OUTPUT_ROOT:-${REPO_ROOT}/.tau/reports/live-validation}"
RUN_ID="${TAU_LIVE_CAPABILITY_RUN_ID:-$(date -u +"%Y%m%d-%H%M%S")}"
TIMEOUT_MS="${TAU_LIVE_CAPABILITY_TIMEOUT_MS:-180000}"
MAX_TURNS="${TAU_LIVE_CAPABILITY_MAX_TURNS:-12}"
CASES_CSV="${TAU_LIVE_CAPABILITY_CASES:-research_openai,blog_anthropic,snake_google,snake_deepseek,blog_xai}"
LONG_OUTPUT_MIN_WORDS="${TAU_LIVE_CAPABILITY_LONG_OUTPUT_MIN_WORDS:-900}"
TAU_BIN="${TAU_LIVE_CAPABILITY_BIN:-${REPO_ROOT}/target/debug/tau-coding-agent}"
SKIP_BUILD="false"

usage() {
  cat <<'EOF'
Usage: live-capability-matrix.sh [options]

Run deterministic live capability scenarios (research/blog/snake) across
configured providers and emit logs + summary under .tau/reports/live-validation.

Options:
  --key-file <path>        Provider key env file (default: .tau/provider-keys.env)
  --output-root <path>     Report root directory (default: .tau/reports/live-validation)
  --run-id <id>            Run identifier prefix (default: UTC timestamp)
  --timeout-ms <ms>        Provider request timeout in milliseconds (default: 180000)
  --max-turns <n>          Max agent turns per case (default: 12)
  --long-output-min-words  Minimum words required for long-output cases (default: 900)
  --cases <csv>            Comma list of case ids to run
  --bin <path>             tau-coding-agent binary path
  --skip-build             Do not build tau-coding-agent if --bin is missing
  --help                   Show this help text
EOF
}

log() {
  printf '%s\n' "$*"
}

require_cmd() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    echo "error: required command not found: $name" >&2
    exit 1
  fi
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
    --bin)
      TAU_BIN="$2"
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

require_cmd grep
require_cmd awk
require_cmd sed

if [[ ! -f "$KEY_FILE" ]]; then
  cat <<MSG
error: provider key file not found: $KEY_FILE
Create it from template:
  cp scripts/dev/provider-keys.env.example .tau/provider-keys.env
  chmod 600 .tau/provider-keys.env
MSG
  exit 1
fi

if ! [[ "$TIMEOUT_MS" =~ ^[0-9]+$ ]]; then
  echo "error: --timeout-ms must be an integer" >&2
  exit 1
fi

if ! [[ "$MAX_TURNS" =~ ^[0-9]+$ ]]; then
  echo "error: --max-turns must be an integer" >&2
  exit 1
fi

if ! [[ "$LONG_OUTPUT_MIN_WORDS" =~ ^[0-9]+$ ]]; then
  echo "error: --long-output-min-words must be an integer" >&2
  exit 1
fi

set -a
# shellcheck disable=SC1090
source "$KEY_FILE"
set +a

OPENAI_KEY="${OPENAI_API_KEY:-${TAU_API_KEY:-}}"
OPENROUTER_KEY="${OPENROUTER_API_KEY:-${TAU_OPENROUTER_API_KEY:-}}"
ANTHROPIC_KEY="${ANTHROPIC_API_KEY:-}"
GOOGLE_KEY="${GEMINI_API_KEY:-${GOOGLE_API_KEY:-}}"

if [[ ! -x "$TAU_BIN" ]]; then
  if [[ "$SKIP_BUILD" == "true" ]]; then
    echo "error: tau binary is not executable and --skip-build is set: $TAU_BIN" >&2
    exit 1
  fi
  log "[build] compiling tau-coding-agent"
  cargo build -p tau-coding-agent --quiet
fi

if [[ ! -x "$TAU_BIN" ]]; then
  echo "error: tau binary is not executable: $TAU_BIN" >&2
  exit 1
fi

RUN_DIR="${OUTPUT_ROOT}/${RUN_ID}-capability-matrix"
mkdir -p "$RUN_DIR"
SUMMARY_TSV="${RUN_DIR}/summary.tsv"
printf "case\tmodel\tstream\trc\tcompletion\ttool_calls\ttool_gate\tartifact\tnotes\n" >"$SUMMARY_TSV"

declare -a CASE_IDS=()
IFS=',' read -r -a CASE_IDS <<<"$CASES_CSV"

case_meta() {
  local case_id="$1"
  case "$case_id" in
    research_openai)
      printf "%s\n" "openai/gpt-4o-mini|openai|research|false|1"
      ;;
    blog_anthropic)
      printf "%s\n" "anthropic/claude-opus-4-6|anthropic|blog|false|1"
      ;;
    snake_google)
      printf "%s\n" "google/gemini-2.5-pro|google|snake|false|1"
      ;;
    snake_deepseek)
      printf "%s\n" "openrouter/deepseek/deepseek-chat-v3.1|openrouter|snake|false|1"
      ;;
    blog_xai)
      printf "%s\n" "openrouter/x-ai/grok-4.1-fast|openrouter|blog|false|1"
      ;;
    research_openai_codex)
      printf "%s\n" "openai/gpt-5.2-codex|openai|research|false|1"
      ;;
    research_openrouter_kimi)
      printf "%s\n" "openrouter/moonshotai/kimi-k2.5|openrouter|research|false|1"
      ;;
    blog_openrouter_minimax)
      printf "%s\n" "openrouter/minimax/minimax-m2.5|openrouter|blog|false|1"
      ;;
    long_output_openai_codex)
      printf "%s\n" "openai/gpt-5.2-codex|openai|long_output|false|1"
      ;;
    stream_openai_research)
      printf "%s\n" "openai/gpt-4o-mini|openai|stream_text|true|0"
      ;;
    stream_anthropic_blog)
      printf "%s\n" "anthropic/claude-opus-4-6|anthropic|stream_text|true|0"
      ;;
    stream_google_snake)
      printf "%s\n" "google/gemini-2.5-pro|google|stream_text|true|0"
      ;;
    stream_openrouter_xai_blog)
      printf "%s\n" "openrouter/x-ai/grok-4.1-fast|openrouter|stream_text|true|0"
      ;;
    session_continuity_openai)
      printf "%s\n" "openai/gpt-4o-mini|openai|session_continuity|false|2"
      ;;
    parallel_tools_openai)
      printf "%s\n" "openai/gpt-4o-mini|openai|parallel_tools|false|3"
      ;;
    *)
      return 1
      ;;
  esac
}

write_prompt() {
  local task="$1"
  local prompt_file="$2"
  case "$task" in
    research)
      cat >"$prompt_file" <<'EOF'
Research the current state of Rust async runtime tradeoffs.
Use tools to create exactly one file named report.md in the current directory.
report.md must contain:
1) a short executive summary,
2) at least 3 source links,
3) a recommendation section.
Do not just describe steps; actually write report.md.
After writing the file, respond with COMPLETE.
EOF
      ;;
    blog)
      cat >"$prompt_file" <<'EOF'
Build a complete static personal blog in the current directory.
Use tools to create index.html, styles.css, and main.js.
Requirements:
1) index links styles.css and main.js,
2) at least 3 sample posts are rendered,
3) include responsive mobile layout and a simple search/filter interaction.
Do not just explain; write the files.
After writing files, respond with COMPLETE.
EOF
      ;;
    snake)
      cat >"$prompt_file" <<'EOF'
Build a playable browser Snake game in the current directory.
Use tools to create index.html and game.js (styles can be inline or separate).
Requirements:
1) keyboard arrow controls,
2) score display,
3) restart on game over.
Do not just explain; write the files.
After writing files, respond with COMPLETE.
EOF
      ;;
    long_output)
      cat >"$prompt_file" <<'EOF'
Create a file named long_output.md in the current directory.
The file must contain:
1) a title line,
2) at least 900 words of prose,
3) a final line that says END-OF-LONG-OUTPUT.
Do not just describe this. Actually write long_output.md via tools.
After writing the file, respond with COMPLETE.
EOF
      ;;
    parallel_tools)
      cat >"$prompt_file" <<'EOF'
Use tools to create all of these files in the current directory:
- alpha.txt
- beta.txt
- gamma.txt
- manifest.json

Requirements:
1) Each text file must contain one short sentence.
2) manifest.json must include keys alpha, beta, and gamma.
3) Perform the file writes directly via tool calls.
After writing files, respond with COMPLETE.
EOF
      ;;
    stream_text)
      cat >"$prompt_file" <<'EOF'
Streaming validation case:
Respond with one short sentence and include the exact token STREAM_CASE_COMPLETE.
Do not call any tools.
EOF
      ;;
    *)
      echo "error: unknown task '$task'" >&2
      return 1
      ;;
  esac
}

provider_args() {
  local provider="$1"
  PROVIDER_ARGS=()
  case "$provider" in
    openai)
      if [[ -z "$OPENAI_KEY" ]]; then
        return 10
      fi
      PROVIDER_ARGS=(
        --openai-api-key "$OPENAI_KEY"
        --openai-auth-mode api-key
        --api-base "${TAU_OPENAI_API_BASE:-https://api.openai.com/v1}"
      )
      ;;
    anthropic)
      if [[ -z "$ANTHROPIC_KEY" ]]; then
        return 10
      fi
      PROVIDER_ARGS=(
        --anthropic-api-key "$ANTHROPIC_KEY"
        --anthropic-auth-mode api-key
      )
      ;;
    google)
      if [[ -z "$GOOGLE_KEY" ]]; then
        return 10
      fi
      PROVIDER_ARGS=(
        --google-api-key "$GOOGLE_KEY"
        --google-auth-mode api-key
      )
      ;;
    openrouter)
      if [[ -z "$OPENROUTER_KEY" ]]; then
        return 10
      fi
      PROVIDER_ARGS=(
        --openai-api-key "$OPENROUTER_KEY"
        --openai-auth-mode api-key
        --api-base "${TAU_OPENROUTER_API_BASE:-https://openrouter.ai/api/v1}"
      )
      ;;
    *)
      echo "error: unknown provider '$provider'" >&2
      return 1
      ;;
  esac
}

check_research_artifacts() {
  local workspace="$1"
  local report="${workspace}/report.md"
  [[ -f "$report" ]] || return 1
  local link_count
  link_count="$(grep -Eo 'https?://[^ )]+' "$report" | wc -l | tr -d ' ')"
  [[ "${link_count:-0}" -ge 3 ]]
}

check_blog_artifacts() {
  local workspace="$1"
  local index="${workspace}/index.html"
  local css="${workspace}/styles.css"
  local js="${workspace}/main.js"
  [[ -f "$index" && -f "$css" && -f "$js" ]] || return 1
  grep -qi "styles.css" "$index"
  grep -qi "main.js" "$index"
}

check_snake_artifacts() {
  local workspace="$1"
  local index="${workspace}/index.html"
  local js="${workspace}/game.js"
  [[ -f "$index" && -f "$js" ]] || return 1
  grep -qi "canvas" "$index"
  grep -Eqi "keydown|arrow" "$js"
}

check_long_output_artifacts() {
  local workspace="$1"
  local long_output="${workspace}/long_output.md"
  [[ -f "$long_output" ]] || return 1
  local word_count
  word_count="$(wc -w < "$long_output" | tr -d ' ')"
  [[ "${word_count:-0}" -ge "$LONG_OUTPUT_MIN_WORDS" ]] || return 1
  grep -q "END-OF-LONG-OUTPUT" "$long_output"
}

check_parallel_tools_artifacts() {
  local workspace="$1"
  [[ -f "${workspace}/alpha.txt" ]] || return 1
  [[ -f "${workspace}/beta.txt" ]] || return 1
  [[ -f "${workspace}/gamma.txt" ]] || return 1
  [[ -f "${workspace}/manifest.json" ]] || return 1
  grep -q '"alpha"' "${workspace}/manifest.json"
  grep -q '"beta"' "${workspace}/manifest.json"
  grep -q '"gamma"' "${workspace}/manifest.json"
}

check_stream_text_artifacts() {
  local log_file="$1"
  grep -q "STREAM_CASE_COMPLETE" "$log_file"
}

write_session_prompt() {
  local phase="$1"
  local prompt_file="$2"
  case "$phase" in
    phase1)
      cat >"$prompt_file" <<'EOF'
Create a file named phase1.txt in the current directory.
The file must include the exact token: SESSION_TOKEN_2243.
After writing phase1.txt, respond with COMPLETE.
EOF
      ;;
    phase2)
      cat >"$prompt_file" <<'EOF'
Using the same session, create a file named phase2.txt in the current directory.
The file must include:
1) the exact token SESSION_TOKEN_2243
2) the phrase resumed-session
You may use tools as needed.
After writing phase2.txt, respond with COMPLETE.
EOF
      ;;
    *)
      echo "error: unknown session phase '$phase'" >&2
      return 1
      ;;
  esac
}

invoke_agent() {
  local workspace="$1"
  local model="$2"
  local stream_mode="$3"
  local session_path="$4"
  local prompt_file="$5"
  local log_file="$6"

  (
    cd "$workspace"
    TAU_ONBOARD_AUTO=false "$TAU_BIN" \
      --model "$model" \
      --max-turns "$MAX_TURNS" \
      --request-timeout-ms "$TIMEOUT_MS" \
      --provider-subscription-strict=true \
      --json-events \
      --stream-output "$stream_mode" \
      --session "$session_path" \
      --prompt-file "$prompt_file" \
      "${PROVIDER_ARGS[@]}"
  ) >"$log_file" 2>&1
}

run_case() {
  local case_id="$1"
  local meta model provider task stream_mode min_tool_calls
  if ! meta="$(case_meta "$case_id")"; then
    echo "error: unsupported case id '$case_id'" >&2
    return 1
  fi
  IFS='|' read -r model provider task stream_mode min_tool_calls <<<"$meta"
  stream_mode="${stream_mode:-false}"
  min_tool_calls="${min_tool_calls:-0}"

  local case_dir="${RUN_DIR}/${case_id}"
  local workspace="${case_dir}/workspace"
  mkdir -p "$workspace"

  local rc completion tool_calls tool_gate artifact notes
  rc=0
  completion="FAIL"
  tool_calls=0
  tool_gate="PASS"
  artifact="FAIL"
  notes=""

  if ! provider_args "$provider"; then
    rc="SKIP"
    completion="SKIP"
    tool_gate="SKIP"
    artifact="SKIP"
    notes="missing_provider_key"
    printf "%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n" \
      "$case_id" "$model" "$stream_mode" "$rc" "$completion" "$tool_calls" "$tool_gate" "$artifact" "$notes" >>"$SUMMARY_TSV"
    return 0
  fi

  log "[run] $case_id ($model)"
  if [[ "$task" == "session_continuity" ]]; then
    local phase1_prompt="${case_dir}/prompt_phase1.txt"
    local phase2_prompt="${case_dir}/prompt_phase2.txt"
    local phase1_log="${case_dir}/phase1.log"
    local phase2_log="${case_dir}/phase2.log"
    local session_path="${case_dir}/session.sqlite"
    local phase1_rc phase2_rc
    phase1_rc=0
    phase2_rc=0

    write_session_prompt phase1 "$phase1_prompt"
    write_session_prompt phase2 "$phase2_prompt"

    if invoke_agent "$workspace" "$model" "$stream_mode" "$session_path" "$phase1_prompt" "$phase1_log"; then
      phase1_rc=0
    else
      phase1_rc=$?
    fi

    if invoke_agent "$workspace" "$model" "$stream_mode" "$session_path" "$phase2_prompt" "$phase2_log"; then
      phase2_rc=0
    else
      phase2_rc=$?
    fi

    if [[ "$phase1_rc" != "0" ]]; then
      rc="$phase1_rc"
    elif [[ "$phase2_rc" != "0" ]]; then
      rc="$phase2_rc"
    fi

    if grep -q '"type":"agent_end"' "$phase1_log" 2>/dev/null && grep -q '"type":"agent_end"' "$phase2_log" 2>/dev/null; then
      completion="PASS"
    fi

    tool_calls="$(( $(grep -c '"type":"tool_execution_start"' "$phase1_log" 2>/dev/null || true) + $(grep -c '"type":"tool_execution_start"' "$phase2_log" 2>/dev/null || true) ))"

    if [[ -f "${workspace}/phase1.txt" && -f "${workspace}/phase2.txt" ]] \
      && grep -q "SESSION_TOKEN_2243" "${workspace}/phase2.txt" \
      && grep -q "resumed-session" "${workspace}/phase2.txt" \
      && [[ -s "$session_path" ]]; then
      artifact="PASS"
    fi
  else
    local prompt_file="${case_dir}/prompt.txt"
    local log_file="${case_dir}/run.log"
    write_prompt "$task" "$prompt_file"

    if invoke_agent "$workspace" "$model" "$stream_mode" "${case_dir}/session.sqlite" "$prompt_file" "$log_file"; then
      rc=0
    else
      rc=$?
    fi

    if grep -q '"type":"agent_end"' "$log_file"; then
      completion="PASS"
    fi

    tool_calls="$(grep -c '"type":"tool_execution_start"' "$log_file" || true)"

    case "$task" in
      research)
        if check_research_artifacts "$workspace"; then artifact="PASS"; fi
        ;;
      blog)
        if check_blog_artifacts "$workspace"; then artifact="PASS"; fi
        ;;
      snake)
        if check_snake_artifacts "$workspace"; then artifact="PASS"; fi
        ;;
      long_output)
        if check_long_output_artifacts "$workspace"; then artifact="PASS"; fi
        ;;
      parallel_tools)
        if check_parallel_tools_artifacts "$workspace"; then artifact="PASS"; fi
        ;;
      stream_text)
        if check_stream_text_artifacts "$log_file"; then artifact="PASS"; fi
        ;;
      *)
        notes="unknown_task_check"
        ;;
    esac
  fi

  if [[ "$tool_calls" -lt "$min_tool_calls" ]]; then
    tool_gate="FAIL"
  fi

  if [[ "$rc" != "0" ]]; then
    notes="non_zero_exit"
  fi
  if [[ "$completion" != "PASS" ]]; then
    if [[ -n "$notes" ]]; then notes="${notes},"; fi
    notes="${notes}missing_agent_end"
  fi
  if [[ "$artifact" != "PASS" ]]; then
    if [[ -n "$notes" ]]; then notes="${notes},"; fi
    notes="${notes}missing_expected_artifacts"
  fi
  if [[ "$tool_gate" != "PASS" ]]; then
    if [[ -n "$notes" ]]; then notes="${notes},"; fi
    notes="${notes}insufficient_tool_calls(min=${min_tool_calls},actual=${tool_calls})"
  fi
  if [[ -z "$notes" ]]; then
    notes="ok"
  fi

  printf "%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n" \
    "$case_id" "$model" "$stream_mode" "$rc" "$completion" "$tool_calls" "$tool_gate" "$artifact" "$notes" >>"$SUMMARY_TSV"
}

for case_id in "${CASE_IDS[@]}"; do
  case_id="$(echo "$case_id" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
  if [[ -z "$case_id" ]]; then
    continue
  fi
  run_case "$case_id"
done

log ""
log "live capability matrix summary:"
cat "$SUMMARY_TSV"
log ""
log "artifacts: $RUN_DIR"
