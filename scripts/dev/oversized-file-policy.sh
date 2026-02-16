#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

EXEMPTIONS_JSON="${REPO_ROOT}/tasks/policies/oversized-file-exemptions.json"
DOC_PATH="docs/guides/oversized-file-policy.md"
DEFAULT_THRESHOLD_LINES=4000
MAX_THRESHOLD_LINES=8000
TODAY_UTC="$(date -u +%Y-%m-%d)"
QUIET_MODE="false"
OUTPUT_MD=""

usage() {
  cat <<'EOF'
Usage: oversized-file-policy.sh [options]

Validate oversized-file exemption metadata against policy requirements.

Options:
  --exemptions-json <path>       Exemption metadata JSON path.
  --default-threshold-lines <n>  Default production-file line threshold (default: 4000).
  --max-threshold-lines <n>      Absolute max exemption threshold (default: 8000).
  --today <YYYY-MM-DD>           Override current UTC date (for deterministic tests).
  --output-md <path>             Write markdown policy summary report.
  --quiet                        Suppress informational output.
  --help                         Show this help text.

Expected JSON shape:
{
  "schema_version": 1,
  "exemptions": [
    {
      "path": "crates/example/src/large_file.rs",
      "threshold_lines": 5200,
      "owner_issue": 1234,
      "rationale": "why this temporary exemption exists",
      "approved_by": "reviewer-handle",
      "approved_at": "2026-02-15",
      "expires_on": "2026-03-15"
    }
  ]
}
EOF
}

require_cmd() {
  local name="$1"
  if ! command -v "${name}" >/dev/null 2>&1; then
    echo "error: required command '${name}' not found" >&2
    exit 1
  fi
}

log_info() {
  if [[ "${QUIET_MODE}" != "true" ]]; then
    echo "$@"
  fi
}

is_iso_date() {
  local value="$1"
  [[ "${value}" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --exemptions-json)
      EXEMPTIONS_JSON="$2"
      shift 2
      ;;
    --default-threshold-lines)
      DEFAULT_THRESHOLD_LINES="$2"
      shift 2
      ;;
    --max-threshold-lines)
      MAX_THRESHOLD_LINES="$2"
      shift 2
      ;;
    --today)
      TODAY_UTC="$2"
      shift 2
      ;;
    --output-md)
      OUTPUT_MD="$2"
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

if ! [[ "${DEFAULT_THRESHOLD_LINES}" =~ ^[0-9]+$ ]] || (( DEFAULT_THRESHOLD_LINES < 1 )); then
  echo "error: --default-threshold-lines must be a positive integer" >&2
  exit 1
fi
if ! [[ "${MAX_THRESHOLD_LINES}" =~ ^[0-9]+$ ]] || (( MAX_THRESHOLD_LINES < DEFAULT_THRESHOLD_LINES )); then
  echo "error: --max-threshold-lines must be an integer >= default threshold" >&2
  exit 1
fi
if ! is_iso_date "${TODAY_UTC}"; then
  echo "error: --today must be in YYYY-MM-DD format" >&2
  exit 1
fi
if [[ ! -f "${EXEMPTIONS_JSON}" ]]; then
  echo "error: exemptions JSON not found: ${EXEMPTIONS_JSON}" >&2
  exit 1
fi

errors=()
warns=()

schema_version="$(jq -r '.schema_version // "null"' "${EXEMPTIONS_JSON}")"
if [[ "${schema_version}" != "1" ]]; then
  errors+=("schema_version must be 1")
fi

if ! jq -e '.exemptions | type == "array"' "${EXEMPTIONS_JSON}" >/dev/null 2>&1; then
  errors+=("exemptions must be an array")
fi

exemption_count="$(jq -r '.exemptions | length' "${EXEMPTIONS_JSON}" 2>/dev/null || printf '0')"
if ! [[ "${exemption_count}" =~ ^[0-9]+$ ]]; then
  errors+=("unable to determine exemptions count")
  exemption_count=0
fi

for ((i = 0; i < exemption_count; i++)); do
  base=".exemptions[${i}]"
  path_value="$(jq -r "${base}.path // \"\"" "${EXEMPTIONS_JSON}")"
  threshold_value="$(jq -r "${base}.threshold_lines // \"\"" "${EXEMPTIONS_JSON}")"
  owner_issue_value="$(jq -r "${base}.owner_issue // \"\"" "${EXEMPTIONS_JSON}")"
  rationale_value="$(jq -r "${base}.rationale // \"\"" "${EXEMPTIONS_JSON}")"
  approved_by_value="$(jq -r "${base}.approved_by // \"\"" "${EXEMPTIONS_JSON}")"
  approved_at_value="$(jq -r "${base}.approved_at // \"\"" "${EXEMPTIONS_JSON}")"
  expires_on_value="$(jq -r "${base}.expires_on // \"\"" "${EXEMPTIONS_JSON}")"

  if [[ -z "${path_value}" ]]; then
    errors+=("exemptions[${i}].path is required")
  fi

  if [[ -n "${path_value}" ]]; then
    repo_file_path="${REPO_ROOT}/${path_value}"
    if [[ ! -f "${repo_file_path}" ]]; then
      errors+=("exemptions[${i}] (${path_value}) path does not exist in repository")
    else
      current_line_count="$(wc -l < "${repo_file_path}" | tr -d '[:space:]')"
      if ! [[ "${current_line_count}" =~ ^[0-9]+$ ]]; then
        errors+=("exemptions[${i}] (${path_value}) current line count could not be determined")
      elif (( current_line_count <= DEFAULT_THRESHOLD_LINES )); then
        errors+=("exemptions[${i}] (${path_value}) is stale because current line count ${current_line_count} is not above default threshold (${DEFAULT_THRESHOLD_LINES})")
      fi
    fi
  fi

  if ! [[ "${threshold_value}" =~ ^[0-9]+$ ]]; then
    errors+=("exemptions[${i}].threshold_lines must be a positive integer")
  else
    if (( threshold_value <= DEFAULT_THRESHOLD_LINES )); then
      errors+=("exemptions[${i}].threshold_lines must be greater than default threshold (${DEFAULT_THRESHOLD_LINES})")
    fi
    if (( threshold_value > MAX_THRESHOLD_LINES )); then
      errors+=("exemptions[${i}].threshold_lines exceeds max threshold (${MAX_THRESHOLD_LINES})")
    fi
  fi

  if ! [[ "${owner_issue_value}" =~ ^[0-9]+$ ]] || (( owner_issue_value < 1 )); then
    errors+=("exemptions[${i}].owner_issue must be a positive integer issue id")
  fi

  if [[ -z "${rationale_value}" ]]; then
    errors+=("exemptions[${i}].rationale is required")
  fi

  if [[ -z "${approved_by_value}" ]]; then
    errors+=("exemptions[${i}].approved_by is required")
  fi

  if ! is_iso_date "${approved_at_value}"; then
    errors+=("exemptions[${i}].approved_at must be YYYY-MM-DD")
  fi

  if ! is_iso_date "${expires_on_value}"; then
    errors+=("exemptions[${i}].expires_on must be YYYY-MM-DD")
  fi

  if is_iso_date "${approved_at_value}" && is_iso_date "${expires_on_value}"; then
    if [[ "${expires_on_value}" < "${approved_at_value}" ]]; then
      errors+=("exemptions[${i}] expires_on must be on/after approved_at")
    fi
    if [[ "${expires_on_value}" < "${TODAY_UTC}" ]]; then
      errors+=("exemptions[${i}] (${path_value}) is expired as of ${TODAY_UTC}")
    fi
  fi
done

duplicate_paths="$(
  jq -r '.exemptions[]?.path // empty' "${EXEMPTIONS_JSON}" \
    | sort \
    | uniq -d \
    | sed '/^$/d' || true
)"
if [[ -n "${duplicate_paths}" ]]; then
  while IFS= read -r duplicate_path; do
    errors+=("duplicate exemption path: ${duplicate_path}")
  done <<<"${duplicate_paths}"
fi

if (( ${#errors[@]} > 0 )); then
  echo "oversized-file policy validation failed:" >&2
  for error in "${errors[@]}"; do
    echo "  - ${error}" >&2
  done
  echo "policy guide: ${DOC_PATH}" >&2
  exit 1
fi

if [[ -n "${OUTPUT_MD}" ]]; then
  mkdir -p "$(dirname "${OUTPUT_MD}")"
  {
    cat <<EOF
# Oversized File Policy Check

- Generated at: $(date -u +"%Y-%m-%dT%H:%M:%SZ")
- Policy guide: \`${DOC_PATH}\`
- Default threshold: ${DEFAULT_THRESHOLD_LINES}
- Max exemption threshold: ${MAX_THRESHOLD_LINES}
- Validation date: ${TODAY_UTC}
- Active exemptions: ${exemption_count}

## Exemptions

| Path | Threshold | Owner Issue | Expires On | Approved By |
| --- | ---: | ---: | --- | --- |
EOF
    if (( exemption_count == 0 )); then
      echo "| _none_ | - | - | - | - |"
    else
      jq -r '.exemptions[] | [.path, (.threshold_lines|tostring), (.owner_issue|tostring), .expires_on, .approved_by] | @tsv' "${EXEMPTIONS_JSON}" \
        | while IFS=$'\t' read -r path threshold owner_issue expires_on approved_by; do
            printf '| %s | %s | %s | %s | %s |\n' \
              "${path}" "${threshold}" "${owner_issue}" "${expires_on}" "${approved_by}"
          done
    fi
  } >"${OUTPUT_MD}"
fi

log_info "oversized-file policy validation passed"
log_info "policy guide: ${DOC_PATH}"
log_info "active exemptions: ${exemption_count}"
if [[ -n "${OUTPUT_MD}" ]]; then
  log_info "report: ${OUTPUT_MD}"
fi
