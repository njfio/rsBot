#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
cd "${REPO_ROOT}"

PARITY_SCRIPT="${REPO_ROOT}/scripts/demo/tool-dispatch-parity.sh"
PERFORMANCE_SCRIPT="${REPO_ROOT}/scripts/dev/tool-split-performance-smoke.sh"
PARITY_JSON="${REPO_ROOT}/ci-artifacts/tool-dispatch-parity.json"
PARITY_MD="${REPO_ROOT}/ci-artifacts/tool-dispatch-parity.md"
PERFORMANCE_JSON="${REPO_ROOT}/ci-artifacts/tool-split-performance-smoke.json"
OUTPUT_JSON="${REPO_ROOT}/tasks/reports/m21-tool-split-validation.json"
OUTPUT_MD="${REPO_ROOT}/tasks/reports/m21-tool-split-validation.md"
SUMMARY_FILE="${GITHUB_STEP_SUMMARY:-}"
PARITY_FIXTURE_JSON=""
PERFORMANCE_FIXTURE_JSON=""
QUIET_MODE="false"

usage() {
  cat <<'USAGE'
Usage: m21-tool-split-validation.sh [options]

Run combined tool-dispatch parity and tool-split performance smoke checks,
and publish a single M21 validation decision artifact.

Options:
  --parity-script <path>               Parity script path.
  --performance-script <path>          Performance smoke script path.
  --parity-json <path>                 Parity JSON artifact path.
  --parity-md <path>                   Parity markdown artifact path.
  --performance-json <path>            Performance JSON artifact path.
  --output-json <path>                 Combined output JSON path.
  --output-md <path>                   Combined output markdown path.
  --summary-file <path>                Summary append target (defaults to GITHUB_STEP_SUMMARY).
  --parity-fixture-json <path>         Fixture JSON passed to parity script.
  --performance-fixture-json <path>    Fixture JSON passed to performance script.
  --quiet                              Suppress informational logs.
  --help                               Show this help text.
USAGE
}

log_info() {
  if [[ "${QUIET_MODE}" != "true" ]]; then
    echo "$@"
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --parity-script)
      PARITY_SCRIPT="$2"
      shift 2
      ;;
    --performance-script)
      PERFORMANCE_SCRIPT="$2"
      shift 2
      ;;
    --parity-json)
      PARITY_JSON="$2"
      shift 2
      ;;
    --parity-md)
      PARITY_MD="$2"
      shift 2
      ;;
    --performance-json)
      PERFORMANCE_JSON="$2"
      shift 2
      ;;
    --output-json)
      OUTPUT_JSON="$2"
      shift 2
      ;;
    --output-md)
      OUTPUT_MD="$2"
      shift 2
      ;;
    --summary-file)
      SUMMARY_FILE="$2"
      shift 2
      ;;
    --parity-fixture-json)
      PARITY_FIXTURE_JSON="$2"
      shift 2
      ;;
    --performance-fixture-json)
      PERFORMANCE_FIXTURE_JSON="$2"
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

if [[ ! -x "${PARITY_SCRIPT}" ]]; then
  echo "error: parity script is missing or not executable: ${PARITY_SCRIPT}" >&2
  exit 1
fi
if [[ ! -x "${PERFORMANCE_SCRIPT}" ]]; then
  echo "error: performance script is missing or not executable: ${PERFORMANCE_SCRIPT}" >&2
  exit 1
fi
if [[ -n "${PARITY_FIXTURE_JSON}" && ! -f "${PARITY_FIXTURE_JSON}" ]]; then
  echo "error: parity fixture JSON not found: ${PARITY_FIXTURE_JSON}" >&2
  exit 1
fi
if [[ -n "${PERFORMANCE_FIXTURE_JSON}" && ! -f "${PERFORMANCE_FIXTURE_JSON}" ]]; then
  echo "error: performance fixture JSON not found: ${PERFORMANCE_FIXTURE_JSON}" >&2
  exit 1
fi
if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for m21-tool-split-validation.sh" >&2
  exit 1
fi

mkdir -p "$(dirname "${PARITY_JSON}")" "$(dirname "${PARITY_MD}")"
mkdir -p "$(dirname "${PERFORMANCE_JSON}")" "$(dirname "${OUTPUT_JSON}")" "$(dirname "${OUTPUT_MD}")"

parity_args=("${PARITY_SCRIPT}" "--output-json" "${PARITY_JSON}" "--output-md" "${PARITY_MD}")
if [[ -n "${PARITY_FIXTURE_JSON}" ]]; then
  parity_args+=("--fixture-json" "${PARITY_FIXTURE_JSON}")
fi
if [[ "${QUIET_MODE}" == "true" ]]; then
  parity_args+=("--quiet")
fi

set +e
parity_output="$("${parity_args[@]}" 2>&1)"
parity_exit=$?
set -e
if [[ -n "${parity_output}" && "${QUIET_MODE}" != "true" ]]; then
  echo "${parity_output}"
fi

perf_args=("${PERFORMANCE_SCRIPT}" "--output-json" "${PERFORMANCE_JSON}")
if [[ -n "${PERFORMANCE_FIXTURE_JSON}" ]]; then
  perf_args+=("--fixture-json" "${PERFORMANCE_FIXTURE_JSON}")
fi
if [[ "${QUIET_MODE}" == "true" ]]; then
  perf_args+=("--quiet")
fi

set +e
performance_output="$("${perf_args[@]}" 2>&1)"
performance_exit=$?
set -e
if [[ -n "${performance_output}" && "${QUIET_MODE}" != "true" ]]; then
  echo "${performance_output}"
fi

if [[ ! -f "${PARITY_JSON}" ]]; then
  echo "error: parity JSON artifact missing: ${PARITY_JSON}" >&2
  exit 1
fi
if [[ ! -f "${PERFORMANCE_JSON}" ]]; then
  echo "error: performance JSON artifact missing: ${PERFORMANCE_JSON}" >&2
  exit 1
fi

parity_failed="$(jq -r '.failed // 0' "${PARITY_JSON}")"
parity_passed="$(jq -r '.passed // 0' "${PARITY_JSON}")"
parity_elapsed_ms="$(jq -r '((.entries // []) | map(.elapsed_ms // 0) | add) // 0' "${PARITY_JSON}")"
performance_status="$(jq -r '.status // "unknown"' "${PERFORMANCE_JSON}")"
performance_total_ms="$(jq -r '.sample_total_ms // 0' "${PERFORMANCE_JSON}")"
performance_baseline_ms="$(jq -r '.baseline_total_ms // 0' "${PERFORMANCE_JSON}")"
performance_drift_ms="$(jq -r '.drift_ms // 0' "${PERFORMANCE_JSON}")"
performance_drift_percent="$(jq -r '.drift_percent // 0' "${PERFORMANCE_JSON}")"
warn_threshold_ms="$(jq -r '.thresholds.warn_total_ms // 0' "${PERFORMANCE_JSON}")"
fail_threshold_ms="$(jq -r '.thresholds.fail_total_ms // 0' "${PERFORMANCE_JSON}")"

reason_codes=()
decision_status="pass"

if [[ "${parity_exit}" -ne 0 || "${parity_failed}" -gt 0 ]]; then
  decision_status="fail"
  reason_codes+=("parity_failed")
fi

if [[ "${performance_status}" == "fail" || "${performance_exit}" -ne 0 ]]; then
  decision_status="fail"
  reason_codes+=("performance_fail")
elif [[ "${performance_status}" == "warn" ]]; then
  if [[ "${decision_status}" != "fail" ]]; then
    decision_status="warn"
  fi
  reason_codes+=("performance_warn")
fi

if [[ ${#reason_codes[@]} -eq 0 ]]; then
  reason_codes+=("all_checks_passed")
fi

reason_codes_json="$(printf '%s\n' "${reason_codes[@]}" | jq -R . | jq -s .)"

generated_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

jq -n \
  --arg generated_at "${generated_at}" \
  --arg parity_json "${PARITY_JSON}" \
  --arg parity_md "${PARITY_MD}" \
  --arg performance_json "${PERFORMANCE_JSON}" \
  --arg decision_status "${decision_status}" \
  --argjson reason_codes "${reason_codes_json}" \
  --argjson parity_payload "$(cat "${PARITY_JSON}")" \
  --argjson performance_payload "$(cat "${PERFORMANCE_JSON}")" \
  '{
    schema_version: 1,
    generated_at: $generated_at,
    artifacts: {
      parity_json: $parity_json,
      parity_md: $parity_md,
      performance_json: $performance_json
    },
    parity: {
      passed: ($parity_payload.passed // 0),
      failed: ($parity_payload.failed // 0),
      total_elapsed_ms: (($parity_payload.entries // []) | map(.elapsed_ms // 0) | add // 0),
      entries: ($parity_payload.entries // [])
    },
    performance: {
      status: ($performance_payload.status // "unknown"),
      sample_total_ms: ($performance_payload.sample_total_ms // 0),
      baseline_total_ms: ($performance_payload.baseline_total_ms // 0),
      drift_ms: ($performance_payload.drift_ms // 0),
      drift_percent: ($performance_payload.drift_percent // 0),
      thresholds: ($performance_payload.thresholds // {})
    },
    decision: {
      status: $decision_status,
      reason_codes: $reason_codes
    }
  }' >"${OUTPUT_JSON}"

{
  echo "# M21 Tool Split Validation Summary"
  echo
  echo "- generated_at: ${generated_at}"
  echo "- decision_status: ${decision_status}"
  echo "- parity_passed: ${parity_passed}"
  echo "- parity_failed: ${parity_failed}"
  echo "- parity_total_elapsed_ms: ${parity_elapsed_ms}"
  echo "- performance_status: ${performance_status}"
  echo "- performance_total_ms: ${performance_total_ms}"
  echo "- performance_baseline_ms: ${performance_baseline_ms}"
  echo "- performance_drift_ms: ${performance_drift_ms}"
  echo "- performance_drift_percent: ${performance_drift_percent}%"
  echo "- performance_warn_threshold_ms: ${warn_threshold_ms}"
  echo "- performance_fail_threshold_ms: ${fail_threshold_ms}"
  echo "- parity_artifact_json: ${PARITY_JSON}"
  echo "- performance_artifact_json: ${PERFORMANCE_JSON}"
  echo "- combined_artifact_json: ${OUTPUT_JSON}"
  echo
  echo "## Decision Reasons"
  for reason in "${reason_codes[@]}"; do
    echo "- ${reason}"
  done
  echo
} >"${OUTPUT_MD}"

summary_tmp="$(mktemp)"
{
  echo "### M21 Tool Split Validation"
  echo "- decision_status: ${decision_status}"
  echo "- parity_failed: ${parity_failed}"
  echo "- performance_status: ${performance_status}"
  echo "- output_json: ${OUTPUT_JSON}"
  echo "- output_md: ${OUTPUT_MD}"
  echo
} >"${summary_tmp}"

cat "${summary_tmp}"
if [[ -n "${SUMMARY_FILE}" ]]; then
  cat "${summary_tmp}" >>"${SUMMARY_FILE}"
fi
rm -f "${summary_tmp}"

if [[ "${decision_status}" == "warn" ]]; then
  echo "::warning::m21 tool split validation reported performance drift warning"
fi
if [[ "${decision_status}" == "fail" ]]; then
  echo "::error::m21 tool split validation failed"
  exit 1
fi

log_info "wrote combined tool split validation artifacts: ${OUTPUT_JSON}, ${OUTPUT_MD}"
