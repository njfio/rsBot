#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
BASELINE_JSON="${REPO_ROOT}/tasks/policies/panic-unsafe-baseline.json"
AUDIT_JSON=""
QUIET_MODE="false"

usage() {
  cat <<'USAGE'
Usage: panic-unsafe-guard.sh [options]

Enforce panic!/unsafe ratchet thresholds against a baseline policy artifact.

Options:
  --repo-root <path>       Repository root (default: auto-detected).
  --baseline-json <path>   Baseline policy JSON path.
  --audit-json <path>      Existing audit JSON (if omitted, audit is generated to temp file).
  --quiet                  Suppress informational output.
  --help                   Show this help text.
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
    --repo-root)
      REPO_ROOT="$2"
      shift 2
      ;;
    --baseline-json)
      BASELINE_JSON="$2"
      shift 2
      ;;
    --audit-json)
      AUDIT_JSON="$2"
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

require_cmd jq

if [[ ! -f "${BASELINE_JSON}" ]]; then
  echo "error: baseline policy not found: ${BASELINE_JSON}" >&2
  exit 1
fi

if [[ -z "${AUDIT_JSON}" ]]; then
  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "${tmp_dir}"' EXIT
  AUDIT_JSON="${tmp_dir}/panic-unsafe-audit.json"
  "${SCRIPT_DIR}/panic-unsafe-audit.sh" --repo-root "${REPO_ROOT}" --output-json "${AUDIT_JSON}" --quiet
fi

if [[ ! -f "${AUDIT_JSON}" ]]; then
  echo "error: audit JSON not found: ${AUDIT_JSON}" >&2
  exit 1
fi

baseline_schema="$(jq -r '.schema_version // "null"' "${BASELINE_JSON}")"
if [[ "${baseline_schema}" != "1" ]]; then
  echo "error: baseline schema_version must be 1" >&2
  exit 1
fi

panic_total_max="$(jq -r '.thresholds.panic_total_max // ""' "${BASELINE_JSON}")"
panic_review_required_max="$(jq -r '.thresholds.panic_review_required_max // ""' "${BASELINE_JSON}")"
unsafe_total_max="$(jq -r '.thresholds.unsafe_total_max // ""' "${BASELINE_JSON}")"
unsafe_review_required_max="$(jq -r '.thresholds.unsafe_review_required_max // ""' "${BASELINE_JSON}")"

for value_name in panic_total_max panic_review_required_max unsafe_total_max unsafe_review_required_max; do
  value="${!value_name}"
  if ! [[ "${value}" =~ ^[0-9]+$ ]]; then
    echo "error: baseline threshold '${value_name}' must be a non-negative integer" >&2
    exit 1
  fi
done

panic_total="$(jq -r '.counters.panic_total // 0' "${AUDIT_JSON}")"
panic_review_required="$(jq -r '.counters.panic_review_required // 0' "${AUDIT_JSON}")"
unsafe_total="$(jq -r '.counters.unsafe_total // 0' "${AUDIT_JSON}")"
unsafe_review_required="$(jq -r '.counters.unsafe_review_required // 0' "${AUDIT_JSON}")"

violations=()
if (( panic_total > panic_total_max )); then
  violations+=("panic_total ${panic_total} > max ${panic_total_max}")
fi
if (( panic_review_required > panic_review_required_max )); then
  violations+=("panic_review_required ${panic_review_required} > max ${panic_review_required_max}")
fi
if (( unsafe_total > unsafe_total_max )); then
  violations+=("unsafe_total ${unsafe_total} > max ${unsafe_total_max}")
fi
if (( unsafe_review_required > unsafe_review_required_max )); then
  violations+=("unsafe_review_required ${unsafe_review_required} > max ${unsafe_review_required_max}")
fi

log_info "panic-unsafe guard"
log_info "baseline_json=${BASELINE_JSON}"
log_info "audit_json=${AUDIT_JSON}"
log_info "panic_total=${panic_total} max=${panic_total_max}"
log_info "panic_review_required=${panic_review_required} max=${panic_review_required_max}"
log_info "unsafe_total=${unsafe_total} max=${unsafe_total_max}"
log_info "unsafe_review_required=${unsafe_review_required} max=${unsafe_review_required_max}"

if (( ${#violations[@]} > 0 )); then
  echo "panic-unsafe guard failed:" >&2
  for violation in "${violations[@]}"; do
    echo "  - ${violation}" >&2
  done
  echo "policy guide: docs/guides/panic-unsafe-policy.md" >&2
  exit 1
fi

log_info "status=pass"
