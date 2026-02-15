#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
cd "${REPO_ROOT}"

BASELINE_MS=3000
WARN_PERCENT=75
FAIL_PERCENT=200
OUTPUT_JSON="${REPO_ROOT}/ci-artifacts/tool-split-performance-smoke.json"
SUMMARY_FILE="${GITHUB_STEP_SUMMARY:-}"
FIXTURE_JSON=""
QUIET_MODE="false"
SKIP_WARMUP="false"

PROBE_NAMES=(
  "registry_dispatch"
  "sessions_history"
  "bash_dry_run"
)

PROBE_COMMANDS=(
  "cargo test -p tau-tools tools::tests::unit_builtin_agent_tool_name_registry_includes_session_tools -- --exact"
  "cargo test -p tau-tools tools::tests::integration_sessions_history_tool_returns_bounded_lineage -- --exact"
  "cargo test -p tau-tools tools::tests::integration_bash_tool_dry_run_validates_without_execution -- --exact"
)

usage() {
  cat <<'EOF'
Usage: tool-split-performance-smoke.sh [options]

Run a lightweight tool-split performance smoke and flag significant drift.

Options:
  --baseline-ms <n>      Baseline total runtime in milliseconds (default: 3000).
  --warn-percent <n>     Warn threshold as +percent over baseline (default: 75).
  --fail-percent <n>     Fail threshold as +percent over baseline (default: 200).
  --output-json <path>   JSON artifact output path.
  --summary-file <path>  Markdown summary append target (defaults to GITHUB_STEP_SUMMARY).
  --fixture-json <path>  Use fixture samples instead of executing cargo probes.
  --skip-warmup          Skip probe warmup pass.
  --quiet                Suppress informational logs.
  --help                 Show this help text.
EOF
}

log_info() {
  if [[ "${QUIET_MODE}" != "true" ]]; then
    echo "$@"
  fi
}

is_uint() {
  [[ "${1}" =~ ^[0-9]+$ ]]
}

now_ms() {
  python3 - <<'PY'
import time
print(int(time.time() * 1000))
PY
}

run_probe() {
  local label="$1"
  local command="$2"
  local phase="$3"
  local tmp_output
  tmp_output="$(mktemp)"
  if ! bash -lc "${command}" >"${tmp_output}" 2>&1; then
    echo "error: probe '${label}' failed during ${phase}" >&2
    cat "${tmp_output}" >&2
    rm -f "${tmp_output}"
    exit 1
  fi
  rm -f "${tmp_output}"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --baseline-ms)
      BASELINE_MS="$2"
      shift 2
      ;;
    --warn-percent)
      WARN_PERCENT="$2"
      shift 2
      ;;
    --fail-percent)
      FAIL_PERCENT="$2"
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
    --skip-warmup)
      SKIP_WARMUP="true"
      shift
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

if ! is_uint "${BASELINE_MS}" || ! is_uint "${WARN_PERCENT}" || ! is_uint "${FAIL_PERCENT}"; then
  echo "error: baseline and percent thresholds must be non-negative integers" >&2
  exit 1
fi

if [[ -n "${FIXTURE_JSON}" && ! -f "${FIXTURE_JSON}" ]]; then
  echo "error: fixture JSON not found: ${FIXTURE_JSON}" >&2
  exit 1
fi

samples_tsv="$(mktemp)"
summary_tmp=""
cleanup() {
  rm -f "${samples_tsv}"
  if [[ -n "${summary_tmp}" ]]; then
    rm -f "${summary_tmp}"
  fi
}
trap cleanup EXIT

if [[ -n "${FIXTURE_JSON}" ]]; then
  python3 - "${FIXTURE_JSON}" >"${samples_tsv}" <<'PY'
import json
import sys

fixture_path = sys.argv[1]
with open(fixture_path, encoding="utf-8") as handle:
    payload = json.load(handle)

samples = payload.get("samples") or []
if samples:
    for index, sample in enumerate(samples, start=1):
        name = sample.get("name") or f"sample_{index}"
        elapsed = sample.get("elapsed_ms")
        if elapsed is None:
            raise SystemExit(f"error: fixture sample '{name}' missing elapsed_ms")
        command = sample.get("command") or "fixture"
        print(f"{name}\t{int(elapsed)}\t{command}")
else:
    total = payload.get("sample_total_ms")
    if total is None:
        raise SystemExit("error: fixture must include either samples[] or sample_total_ms")
    print(f"fixture_total\t{int(total)}\tfixture")
PY
else
  if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo is required for live probe execution" >&2
    exit 1
  fi

  if [[ "${SKIP_WARMUP}" != "true" ]]; then
    for index in "${!PROBE_NAMES[@]}"; do
      run_probe "${PROBE_NAMES[${index}]}" "${PROBE_COMMANDS[${index}]}" "warmup"
    done
  fi

  for index in "${!PROBE_NAMES[@]}"; do
    started_ms="$(now_ms)"
    run_probe "${PROBE_NAMES[${index}]}" "${PROBE_COMMANDS[${index}]}" "measurement"
    finished_ms="$(now_ms)"
    elapsed_ms="$((finished_ms - started_ms))"
    printf '%s\t%s\t%s\n' \
      "${PROBE_NAMES[${index}]}" \
      "${elapsed_ms}" \
      "${PROBE_COMMANDS[${index}]}" >>"${samples_tsv}"
    log_info "probe=${PROBE_NAMES[${index}]} elapsed_ms=${elapsed_ms}"
  done
fi

if [[ ! -s "${samples_tsv}" ]]; then
  echo "error: no probe samples were collected" >&2
  exit 1
fi

total_ms=0
while IFS=$'\t' read -r _name elapsed_ms _command; do
  total_ms="$((total_ms + elapsed_ms))"
done <"${samples_tsv}"

warn_ms="$((BASELINE_MS + (BASELINE_MS * WARN_PERCENT / 100)))"
fail_ms="$((BASELINE_MS + (BASELINE_MS * FAIL_PERCENT / 100)))"
drift_ms="$((total_ms - BASELINE_MS))"
drift_percent="$(
  python3 - "${drift_ms}" "${BASELINE_MS}" <<'PY'
import sys

drift = int(sys.argv[1])
baseline = int(sys.argv[2])
if baseline == 0:
    print("0.00")
else:
    print(f"{(drift / baseline) * 100.0:.2f}")
PY
)"

status="pass"
exit_code=0
if (( total_ms > fail_ms )); then
  status="fail"
  exit_code=1
elif (( total_ms > warn_ms )); then
  status="warn"
fi

mkdir -p "$(dirname "${OUTPUT_JSON}")"
generated_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

python3 - \
  "${samples_tsv}" \
  "${OUTPUT_JSON}" \
  "${generated_at}" \
  "${BASELINE_MS}" \
  "${WARN_PERCENT}" \
  "${FAIL_PERCENT}" \
  "${warn_ms}" \
  "${fail_ms}" \
  "${total_ms}" \
  "${drift_ms}" \
  "${drift_percent}" \
  "${status}" <<'PY'
import json
import sys

(
    samples_path,
    output_path,
    generated_at,
    baseline_ms,
    warn_percent,
    fail_percent,
    warn_ms,
    fail_ms,
    total_ms,
    drift_ms,
    drift_percent,
    status,
) = sys.argv[1:]

samples = []
with open(samples_path, encoding="utf-8") as handle:
    for row in handle:
        row = row.rstrip("\n")
        if not row:
            continue
        name, elapsed_ms, command = row.split("\t", 2)
        samples.append(
            {
                "name": name,
                "elapsed_ms": int(elapsed_ms),
                "command": command,
            }
        )

payload = {
    "schema_version": 1,
    "generated_at": generated_at,
    "baseline_total_ms": int(baseline_ms),
    "thresholds": {
        "warn_percent": int(warn_percent),
        "fail_percent": int(fail_percent),
        "warn_total_ms": int(warn_ms),
        "fail_total_ms": int(fail_ms),
    },
    "sample_total_ms": int(total_ms),
    "drift_ms": int(drift_ms),
    "drift_percent": float(drift_percent),
    "status": status,
    "samples": samples,
}

with open(output_path, "w", encoding="utf-8") as handle:
    json.dump(payload, handle, indent=2)
    handle.write("\n")
PY

summary_tmp="$(mktemp)"
{
  echo "### Tool Split Performance Smoke"
  echo "- status: ${status}"
  echo "- baseline_total_ms: ${BASELINE_MS}"
  echo "- measured_total_ms: ${total_ms}"
  echo "- drift_ms: ${drift_ms}"
  echo "- drift_percent: ${drift_percent}%"
  echo "- warn_threshold_ms: ${warn_ms} (+${WARN_PERCENT}%)"
  echo "- fail_threshold_ms: ${fail_ms} (+${FAIL_PERCENT}%)"
  echo
  echo "| Probe | Elapsed (ms) |"
  echo "| --- | ---: |"
  while IFS=$'\t' read -r name elapsed_ms _command; do
    echo "| ${name} | ${elapsed_ms} |"
  done <"${samples_tsv}"
  echo
} >"${summary_tmp}"

cat "${summary_tmp}"
if [[ -n "${SUMMARY_FILE}" ]]; then
  cat "${summary_tmp}" >>"${SUMMARY_FILE}"
fi

if [[ "${status}" == "warn" ]]; then
  echo "::warning::tool split performance smoke drift detected (total=${total_ms}ms, baseline=${BASELINE_MS}ms, warn>${warn_ms}ms)"
fi
if [[ "${status}" == "fail" ]]; then
  echo "::error::tool split performance smoke threshold exceeded (total=${total_ms}ms, baseline=${BASELINE_MS}ms, fail>${fail_ms}ms)"
fi

log_info "wrote tool split performance smoke artifact: ${OUTPUT_JSON}"
exit "${exit_code}"
