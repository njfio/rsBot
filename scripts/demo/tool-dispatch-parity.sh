#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
cd "${REPO_ROOT}"

OUTPUT_MD="${REPO_ROOT}/ci-artifacts/tool-dispatch-parity.md"
OUTPUT_JSON="${REPO_ROOT}/ci-artifacts/tool-dispatch-parity.json"
SUMMARY_FILE="${GITHUB_STEP_SUMMARY:-}"
FIXTURE_JSON=""
QUIET_MODE="false"

PARITY_BEHAVIORS=(
  "Built-in registry contains split session tools"
  "Session history dispatch returns bounded lineage"
  "HTTP tool dispatch preserves JSON request/response contract"
  "Bash tool dispatch preserves dry-run policy contract"
  "Memory tool dispatch preserves write/read round-trip contract"
)

PARITY_COMMANDS=(
  "cargo test -p tau-tools tools::tests::unit_builtin_agent_tool_name_registry_includes_session_tools -- --exact"
  "cargo test -p tau-tools tools::tests::integration_sessions_history_tool_returns_bounded_lineage -- --exact"
  "cargo test -p tau-tools tools::tests::functional_http_tool_posts_json_and_returns_structured_payload -- --exact"
  "cargo test -p tau-tools tools::tests::integration_bash_tool_dry_run_validates_without_execution -- --exact"
  "cargo test -p tau-tools tools::tests::functional_memory_write_and_read_tools_round_trip_record -- --exact"
)

PARITY_PASS_CRITERIA=(
  "Test exits 0 and asserts all expected built-in tool names are present."
  "Test exits 0 and asserts lineage entry count/order invariants."
  "Test exits 0 and asserts HTTP JSON body and structured response invariants."
  "Test exits 0 and asserts dry-run policy response metadata invariants."
  "Test exits 0 and asserts persisted memory record can be read back correctly."
)

usage() {
  cat <<'EOF'
Usage: tool-dispatch-parity.sh [options]

Run a before/after dispatch behavior parity checklist against tau-tools tests.

Options:
  --output-md <path>     Markdown checklist output path.
  --output-json <path>   JSON checklist output path.
  --summary-file <path>  Markdown summary append target (defaults to GITHUB_STEP_SUMMARY).
  --fixture-json <path>  Use fixture results instead of executing cargo tests.
  --quiet                Suppress informational logs.
  --help                 Show this help text.
EOF
}

log_info() {
  if [[ "${QUIET_MODE}" != "true" ]]; then
    echo "$@"
  fi
}

now_ms() {
  python3 - <<'PY'
import time
print(int(time.time() * 1000))
PY
}

run_check_command() {
  local behavior="$1"
  local command="$2"
  local output_file="$3"
  if ! bash -lc "${command}" >"${output_file}" 2>&1; then
    echo "FAIL parity behavior: ${behavior}" >&2
    tail -n 120 "${output_file}" >&2
    return 1
  fi
  return 0
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --output-md)
      OUTPUT_MD="$2"
      shift 2
      ;;
    --output-json)
      OUTPUT_JSON="$2"
      shift 2
      ;;
    --summary-file)
      SUMMARY_FILE="$2"
      shift 2
      ;;
    --fixture-json)
      FIXTURE_JSON="$2"
      shift 2
      ;;
    --quiet)
      QUIET_MODE="true"
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument '$1'" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ -n "${FIXTURE_JSON}" && ! -f "${FIXTURE_JSON}" ]]; then
  echo "error: fixture JSON not found: ${FIXTURE_JSON}" >&2
  exit 1
fi

if [[ -z "${FIXTURE_JSON}" ]] && ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo is required for live parity execution" >&2
  exit 1
fi

results_tsv="$(mktemp)"
summary_tmp="$(mktemp)"
cleanup() {
  rm -f "${results_tsv}" "${summary_tmp}"
}
trap cleanup EXIT

if [[ -n "${FIXTURE_JSON}" ]]; then
  python3 - "${FIXTURE_JSON}" >"${results_tsv}" <<'PY'
import json
import sys

fixture_path = sys.argv[1]
with open(fixture_path, encoding="utf-8") as handle:
    payload = json.load(handle)

entries = payload.get("entries") or []
if not entries:
    raise SystemExit("error: fixture entries[] is required")

for entry in entries:
    behavior = entry.get("behavior")
    command = entry.get("command")
    pass_criteria = entry.get("pass_criteria")
    status = entry.get("status")
    elapsed_ms = entry.get("elapsed_ms", 0)
    exit_code = entry.get("exit_code", 0 if status == "pass" else 1)
    if not behavior or not command or not pass_criteria or not status:
        raise SystemExit("error: fixture entry is missing required fields")
    print(
        f"{behavior}\t{command}\t{pass_criteria}\t{status}\t{int(elapsed_ms)}\t{int(exit_code)}"
    )
PY
else
  for index in "${!PARITY_BEHAVIORS[@]}"; do
    behavior="${PARITY_BEHAVIORS[${index}]}"
    command="${PARITY_COMMANDS[${index}]}"
    pass_criteria="${PARITY_PASS_CRITERIA[${index}]}"
    probe_output="$(mktemp)"
    started_ms="$(now_ms)"
    if run_check_command "${behavior}" "${command}" "${probe_output}"; then
      status="pass"
      exit_code=0
    else
      status="fail"
      exit_code=1
    fi
    finished_ms="$(now_ms)"
    elapsed_ms="$((finished_ms - started_ms))"
    rm -f "${probe_output}"
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' \
      "${behavior}" \
      "${command}" \
      "${pass_criteria}" \
      "${status}" \
      "${elapsed_ms}" \
      "${exit_code}" >>"${results_tsv}"
    log_info "parity_behavior=\"${behavior}\" status=${status} elapsed_ms=${elapsed_ms}"
  done
fi

if [[ ! -s "${results_tsv}" ]]; then
  echo "error: no parity results were collected" >&2
  exit 1
fi

generated_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
passed_count=0
failed_count=0
while IFS=$'\t' read -r _behavior _command _pass_criteria status _elapsed _exit_code; do
  if [[ "${status}" == "pass" ]]; then
    passed_count="$((passed_count + 1))"
  else
    failed_count="$((failed_count + 1))"
  fi
done <"${results_tsv}"

mkdir -p "$(dirname "${OUTPUT_MD}")" "$(dirname "${OUTPUT_JSON}")"

{
  echo "# Tool Dispatch Before/After Parity Checklist"
  echo
  echo "- generated_at: ${generated_at}"
  echo "- passed: ${passed_count}"
  echo "- failed: ${failed_count}"
  echo
  echo "| Behavior | Command | Pass Criteria | Status | Elapsed (ms) |"
  echo "| --- | --- | --- | --- | ---: |"
  while IFS=$'\t' read -r behavior command pass_criteria status elapsed_ms _exit_code; do
    status_label="PASS"
    if [[ "${status}" != "pass" ]]; then
      status_label="FAIL"
    fi
    echo "| ${behavior} | ${command} | ${pass_criteria} | ${status_label} | ${elapsed_ms} |"
  done <"${results_tsv}"
  echo
} >"${OUTPUT_MD}"

python3 - "${results_tsv}" "${OUTPUT_JSON}" "${generated_at}" <<'PY'
import json
import sys

results_path, output_path, generated_at = sys.argv[1:]

entries = []
passed = 0
failed = 0
with open(results_path, encoding="utf-8") as handle:
    for row in handle:
        behavior, command, pass_criteria, status, elapsed_ms, exit_code = row.rstrip("\n").split(
            "\t", 5
        )
        payload = {
            "behavior": behavior,
            "command": command,
            "pass_criteria": pass_criteria,
            "status": status,
            "elapsed_ms": int(elapsed_ms),
            "exit_code": int(exit_code),
        }
        entries.append(payload)
        if status == "pass":
            passed += 1
        else:
            failed += 1

report = {
    "schema_version": 1,
    "generated_at": generated_at,
    "passed": passed,
    "failed": failed,
    "entries": entries,
}

with open(output_path, "w", encoding="utf-8") as handle:
    json.dump(report, handle, indent=2)
    handle.write("\n")
PY

{
  echo "### Tool Dispatch Parity Checklist"
  echo "- passed: ${passed_count}"
  echo "- failed: ${failed_count}"
  echo "- markdown_artifact: ${OUTPUT_MD}"
  echo "- json_artifact: ${OUTPUT_JSON}"
  echo
} >"${summary_tmp}"

cat "${summary_tmp}"
if [[ -n "${SUMMARY_FILE}" ]]; then
  cat "${summary_tmp}" >>"${SUMMARY_FILE}"
fi

if [[ "${failed_count}" -gt 0 ]]; then
  echo "::error::tool dispatch parity checklist detected ${failed_count} failed behavior checks"
  exit 1
fi

log_info "wrote tool dispatch parity checklist artifacts: ${OUTPUT_MD}, ${OUTPUT_JSON}"
