#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
MATRIX_SCRIPT="${REPO_ROOT}/scripts/dev/live-capability-matrix.sh"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

keys_file="${tmp_dir}/provider-keys.env"
cat >"${keys_file}" <<'EOF'
OPENAI_API_KEY=test-openai
ANTHROPIC_API_KEY=test-anthropic
GEMINI_API_KEY=test-google
OPENROUTER_API_KEY=test-openrouter
EOF

fake_bin="${tmp_dir}/fake-tau-coding-agent.sh"
cat >"${fake_bin}" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

prompt_file=""
session_file=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --prompt-file)
      prompt_file="$2"
      shift 2
      ;;
    --session)
      session_file="$2"
      shift 2
      ;;
    --model|--max-turns|--request-timeout-ms|--stream-output|--api-base|--openai-api-key|--openai-auth-mode|--anthropic-api-key|--anthropic-auth-mode|--google-api-key|--google-auth-mode|--provider-subscription-strict)
      shift 2
      ;;
    --json-events)
      shift
      ;;
    *)
      shift
      ;;
  esac
done

if [[ -z "${prompt_file}" || ! -f "${prompt_file}" ]]; then
  echo '{"type":"agent_start"}'
  echo '{"type":"agent_end","new_messages":0}'
  exit 0
fi

if [[ -n "${session_file}" ]]; then
  mkdir -p "$(dirname "${session_file}")"
  printf '{"event":"fake-session-append"}\n' >>"${session_file}"
fi

prompt_text="$(cat "${prompt_file}")"
emit_stream_marker="false"
if echo "${prompt_text}" | grep -qi "report.md"; then
  cat >report.md <<'REPORT'
# Rust Async Runtime Tradeoffs

- https://tokio.rs
- https://docs.rs/async-std
- https://docs.rs/smol
REPORT
elif echo "${prompt_text}" | grep -qi "index.html, styles.css, and main.js"; then
  cat >index.html <<'HTML'
<html><head><link rel="stylesheet" href="styles.css"></head><body><script src="main.js"></script></body></html>
HTML
  cat >styles.css <<'CSS'
body { font-family: sans-serif; }
CSS
  cat >main.js <<'JS'
console.log("blog-ready");
JS
elif echo "${prompt_text}" | grep -qi "Snake game"; then
  cat >index.html <<'HTML'
<html><body><canvas id="game"></canvas><script src="game.js"></script></body></html>
HTML
  cat >game.js <<'JS'
document.addEventListener("keydown", () => {});
JS
elif echo "${prompt_text}" | grep -qi "long_output.md"; then
  {
    echo "# Long Output"
    for i in $(seq 1 950); do
      printf "word%s " "$i"
    done
    echo ""
    echo "END-OF-LONG-OUTPUT"
  } >long_output.md
elif echo "${prompt_text}" | grep -qi "phase1.txt"; then
  cat >phase1.txt <<'TXT'
SESSION_TOKEN_2243 phase1 complete
TXT
elif echo "${prompt_text}" | grep -qi "phase2.txt"; then
  cat >phase2.txt <<'TXT'
SESSION_TOKEN_2243 resumed-session phase2 complete
TXT
elif echo "${prompt_text}" | grep -qi "alpha.txt"; then
  cat >alpha.txt <<'TXT'
alpha file
TXT
  cat >beta.txt <<'TXT'
beta file
TXT
  cat >gamma.txt <<'TXT'
gamma file
TXT
  cat >manifest.json <<'JSON'
{"alpha":"alpha.txt","beta":"beta.txt","gamma":"gamma.txt"}
JSON
elif echo "${prompt_text}" | grep -q "STREAM_CASE_COMPLETE"; then
  emit_stream_marker="true"
fi

echo '{"type":"agent_start"}'
echo '{"type":"tool_execution_start","tool_name":"write","tool_call_id":"call_1","arguments":{"path":"artifact"}}'
echo '{"type":"tool_execution_end","tool_name":"write","tool_call_id":"call_1","is_error":false}'
echo '{"type":"tool_execution_start","tool_name":"write","tool_call_id":"call_2","arguments":{"path":"artifact2"}}'
echo '{"type":"tool_execution_end","tool_name":"write","tool_call_id":"call_2","is_error":false}'
echo '{"type":"tool_execution_start","tool_name":"write","tool_call_id":"call_3","arguments":{"path":"artifact3"}}'
echo '{"type":"tool_execution_end","tool_name":"write","tool_call_id":"call_3","is_error":false}'
echo '{"type":"agent_end","new_messages":2}'
if [[ "${emit_stream_marker}" == "true" ]]; then
  echo "STREAM_CASE_COMPLETE"
fi
echo "COMPLETE"
EOF
chmod +x "${fake_bin}"

output_root="${tmp_dir}/reports"

TAU_PROVIDER_KEYS_FILE="${keys_file}" \
TAU_LIVE_CAPABILITY_BIN="${fake_bin}" \
TAU_LIVE_CAPABILITY_OUTPUT_ROOT="${output_root}" \
TAU_LIVE_CAPABILITY_RUN_ID="test-run" \
TAU_LIVE_CAPABILITY_CASES="research_openai_codex,research_openrouter_kimi,blog_openrouter_minimax,long_output_openai_codex,stream_openai_research,session_continuity_openai,parallel_tools_openai" \
"${MATRIX_SCRIPT}" --skip-build

summary_path="${output_root}/test-run-capability-matrix/summary.tsv"
if [[ ! -f "${summary_path}" ]]; then
  echo "expected summary file not found: ${summary_path}" >&2
  exit 1
fi

line_count="$(wc -l < "${summary_path}" | tr -d ' ')"
if [[ "${line_count}" != "8" ]]; then
  echo "expected 8 summary lines (header + 7 cases), got ${line_count}" >&2
  cat "${summary_path}" >&2
  exit 1
fi

assert_row() {
  local case_id="$1"
  local expected_artifact="$2"
  local min_tools="$3"
  local row
  row="$(awk -F '\t' -v id="${case_id}" '$1 == id { print $0 }' "${summary_path}")"
  if [[ -z "${row}" ]]; then
    echo "missing row for case ${case_id}" >&2
    cat "${summary_path}" >&2
    exit 1
  fi
  local completion tool_calls tool_gate artifact
  completion="$(echo "${row}" | awk -F '\t' '{ print $5 }')"
  tool_calls="$(echo "${row}" | awk -F '\t' '{ print $6 }')"
  tool_gate="$(echo "${row}" | awk -F '\t' '{ print $7 }')"
  artifact="$(echo "${row}" | awk -F '\t' '{ print $8 }')"
  if [[ "${completion}" != "PASS" ]]; then
    echo "expected completion PASS for ${case_id}, got ${completion}" >&2
    echo "${row}" >&2
    exit 1
  fi
  if [[ "${tool_gate}" != "PASS" ]]; then
    echo "expected tool_gate PASS for ${case_id}, got ${tool_gate}" >&2
    echo "${row}" >&2
    exit 1
  fi
  if [[ "${artifact}" != "${expected_artifact}" ]]; then
    echo "expected artifact ${expected_artifact} for ${case_id}, got ${artifact}" >&2
    echo "${row}" >&2
    exit 1
  fi
  if [[ "${tool_calls}" -lt "${min_tools}" ]]; then
    echo "expected at least ${min_tools} tool calls for ${case_id}, got ${tool_calls}" >&2
    echo "${row}" >&2
    exit 1
  fi
}

assert_row "research_openai_codex" "PASS" 1
assert_row "research_openrouter_kimi" "PASS" 1
assert_row "blog_openrouter_minimax" "PASS" 1
assert_row "long_output_openai_codex" "PASS" 1
assert_row "stream_openai_research" "PASS" 0
assert_row "session_continuity_openai" "PASS" 2
assert_row "parallel_tools_openai" "PASS" 3

echo "live-capability-matrix tests passed"
