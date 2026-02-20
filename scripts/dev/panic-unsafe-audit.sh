#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
OUTPUT_JSON="${REPO_ROOT}/.tau/reports/quality/panic-unsafe-audit.json"
QUIET_MODE="false"

usage() {
  cat <<'USAGE'
Usage: panic-unsafe-audit.sh [options]

Generate deterministic panic!/unsafe usage inventory for crate source.

Options:
  --repo-root <path>      Repository root to scan (default: auto-detected).
  --output-json <path>    JSON output file path.
  --quiet                 Suppress informational output.
  --help                  Show this help text.
USAGE
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

is_test_path() {
  local file="$1"
  if [[ "${file}" =~ (^|/)(tests?|benches|examples)(/|$) ]]; then
    return 0
  fi
  if [[ "${file}" =~ (^|/)src/tests(/|$) ]]; then
    return 0
  fi
  if [[ "${file}" =~ tests\.rs$ ]] || [[ "${file}" =~ _test\.rs$ ]]; then
    return 0
  fi
  return 1
}

classify_occurrence() {
  local file="$1"
  local line="$2"

  if is_test_path "${file}"; then
    printf 'path_test'
    return 0
  fi

  local cfg_line=""
  cfg_line="$(rg -n '^\s*#\s*\[cfg\(test\)\]' "${REPO_ROOT}/${file}" 2>/dev/null | head -n1 | cut -d: -f1 || true)"
  if [[ -n "${cfg_line}" ]] && [[ "${cfg_line}" =~ ^[0-9]+$ ]] && (( line >= cfg_line )); then
    printf 'cfg_test_module'
    return 0
  fi

  local start_line=1
  if (( line > 120 )); then
    start_line=$((line - 120))
  fi
  if sed -n "${start_line},${line}p" "${REPO_ROOT}/${file}" | rg -q '#\s*\[(tokio::)?test\b|#\s*\[rstest\b'; then
    printf 'inline_test'
    return 0
  fi

  printf 'review_required'
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo-root)
      REPO_ROOT="$2"
      shift 2
      ;;
    --output-json)
      OUTPUT_JSON="$2"
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

require_cmd rg
require_cmd jq
require_cmd sed
require_cmd awk

if [[ ! -d "${REPO_ROOT}" ]]; then
  echo "error: repo root not found: ${REPO_ROOT}" >&2
  exit 1
fi

pushd "${REPO_ROOT}" >/dev/null

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

panic_matches="${tmp_dir}/panic_matches.txt"
unsafe_matches="${tmp_dir}/unsafe_matches.txt"
occurrences="${tmp_dir}/occurrences.tsv"
: > "${panic_matches}"
: > "${unsafe_matches}"
: > "${occurrences}"

rg -n 'panic!\(' crates --glob '!**/target/**' > "${panic_matches}" || true
rg -n -e '\bunsafe\s*\{' -e '\bunsafe\s+fn\b' -e '\bunsafe\s+impl\b' -e '\bunsafe\s+trait\b' -e '\bunsafe\s+extern\b' crates --glob '!**/target/**' > "${unsafe_matches}" || true

while IFS=: read -r file line _; do
  [[ -n "${file}" ]] || continue
  bucket="$(classify_occurrence "${file}" "${line}")"
  printf 'panic\t%s\t%s\t%s\n' "${file}" "${line}" "${bucket}" >> "${occurrences}"
done < "${panic_matches}"

while IFS=: read -r file line _; do
  [[ -n "${file}" ]] || continue
  bucket="$(classify_occurrence "${file}" "${line}")"
  printf 'unsafe\t%s\t%s\t%s\n' "${file}" "${line}" "${bucket}" >> "${occurrences}"
done < "${unsafe_matches}"

panic_total="$(awk -F'\t' '$1=="panic"{c++} END{print c+0}' "${occurrences}")"
panic_review_required="$(awk -F'\t' '$1=="panic" && $4=="review_required"{c++} END{print c+0}' "${occurrences}")"
panic_cfg_test_module="$(awk -F'\t' '$1=="panic" && $4=="cfg_test_module"{c++} END{print c+0}' "${occurrences}")"
panic_inline_test="$(awk -F'\t' '$1=="panic" && $4=="inline_test"{c++} END{print c+0}' "${occurrences}")"
panic_path_test="$(awk -F'\t' '$1=="panic" && $4=="path_test"{c++} END{print c+0}' "${occurrences}")"

unsafe_total="$(awk -F'\t' '$1=="unsafe"{c++} END{print c+0}' "${occurrences}")"
unsafe_review_required="$(awk -F'\t' '$1=="unsafe" && $4=="review_required"{c++} END{print c+0}' "${occurrences}")"
unsafe_cfg_test_module="$(awk -F'\t' '$1=="unsafe" && $4=="cfg_test_module"{c++} END{print c+0}' "${occurrences}")"
unsafe_inline_test="$(awk -F'\t' '$1=="unsafe" && $4=="inline_test"{c++} END{print c+0}' "${occurrences}")"
unsafe_path_test="$(awk -F'\t' '$1=="unsafe" && $4=="path_test"{c++} END{print c+0}' "${occurrences}")"

panic_by_file_json="$({ awk -F'\t' '$1=="panic" { key=$2"\t"$4; count[key]++ } END { for (k in count) { split(k, parts, "\t"); printf "%s\t%s\t%d\n", parts[1], parts[2], count[k] } }' "${occurrences}" | sort; } | jq -R -s 'split("\n") | map(select(length > 0) | split("\t") | {path: .[0], bucket: .[1], count: (.[2] | tonumber)})')"

unsafe_by_file_json="$({ awk -F'\t' '$1=="unsafe" { key=$2"\t"$4; count[key]++ } END { for (k in count) { split(k, parts, "\t"); printf "%s\t%s\t%d\n", parts[1], parts[2], count[k] } }' "${occurrences}" | sort; } | jq -R -s 'split("\n") | map(select(length > 0) | split("\t") | {path: .[0], bucket: .[1], count: (.[2] | tonumber)})')"

generated_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
mkdir -p "$(dirname "${OUTPUT_JSON}")"

jq -n \
  --arg generated_at "${generated_at}" \
  --arg repo_root "${REPO_ROOT}" \
  --argjson panic_total "${panic_total}" \
  --argjson panic_review_required "${panic_review_required}" \
  --argjson panic_cfg_test_module "${panic_cfg_test_module}" \
  --argjson panic_inline_test "${panic_inline_test}" \
  --argjson panic_path_test "${panic_path_test}" \
  --argjson unsafe_total "${unsafe_total}" \
  --argjson unsafe_review_required "${unsafe_review_required}" \
  --argjson unsafe_cfg_test_module "${unsafe_cfg_test_module}" \
  --argjson unsafe_inline_test "${unsafe_inline_test}" \
  --argjson unsafe_path_test "${unsafe_path_test}" \
  --argjson panic_by_file "${panic_by_file_json}" \
  --argjson unsafe_by_file "${unsafe_by_file_json}" \
  '{
    generated_at: $generated_at,
    repo_root: $repo_root,
    counters: {
      panic_total: $panic_total,
      panic_review_required: $panic_review_required,
      panic_cfg_test_module: $panic_cfg_test_module,
      panic_inline_test: $panic_inline_test,
      panic_path_test: $panic_path_test,
      unsafe_total: $unsafe_total,
      unsafe_review_required: $unsafe_review_required,
      unsafe_cfg_test_module: $unsafe_cfg_test_module,
      unsafe_inline_test: $unsafe_inline_test,
      unsafe_path_test: $unsafe_path_test
    },
    panic_by_file: $panic_by_file,
    unsafe_by_file: $unsafe_by_file
  }' > "${OUTPUT_JSON}"

log_info "panic-unsafe audit"
log_info "repo_root=${REPO_ROOT}"
log_info "output_json=${OUTPUT_JSON}"
log_info "panic_total=${panic_total} review_required=${panic_review_required}"
log_info "unsafe_total=${unsafe_total} review_required=${unsafe_review_required}"

popd >/dev/null
