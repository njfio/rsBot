#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
OUTPUT_JSON="${REPO_ROOT}/.tau/reports/quality/panic-unsafe-audit.json"
QUIET_MODE="false"
declare -A TEST_CONTEXT_CACHE=()

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

test_context_bucket_for_line() {
  local file="$1"
  local line_number="$2"
  local absolute_path="${REPO_ROOT}/${file}"
  local cache_key="${absolute_path}:${line_number}"

  if [[ -n "${TEST_CONTEXT_CACHE[${cache_key}]+x}" ]]; then
    printf '%s' "${TEST_CONTEXT_CACHE[${cache_key}]}"
    return
  fi

  if [[ ! -f "${absolute_path}" ]]; then
    TEST_CONTEXT_CACHE["${cache_key}"]="none"
    printf 'none'
    return
  fi

  local result
  result="$(
    awk -v target_line="${line_number}" '
      function has_cfg_test_attribute(text) {
        return text ~ /#\[cfg\([^]]*test[^]]*\)\]/
      }

      function has_inline_test_attribute(text) {
        return text ~ /#\[(tokio::)?test[^]]*\]/ || text ~ /#\[rstest[^]]*\]/
      }

      BEGIN {
        depth = 0
        pending_cfg_test = 0
        pending_inline_test = 0
        line_bucket = "none"
      }

      {
        line = $0

        if (line !~ /#\[cfg_attr\(/ && has_cfg_test_attribute(line)) {
          pending_cfg_test = 1
        }
        if (line !~ /#\[cfg_attr\(/ && has_inline_test_attribute(line)) {
          pending_inline_test = 1
        }

        current_cfg = 0
        current_inline = 0
        for (d = 1; d <= depth; d++) {
          if (cfg_depth[d] == 1) {
            current_cfg = 1
          }
          if (inline_depth[d] == 1) {
            current_inline = 1
          }
        }

        if (NR == target_line) {
          if (current_cfg == 1) {
            line_bucket = "cfg_test_module"
          } else if (current_inline == 1) {
            line_bucket = "inline_test"
          } else {
            line_bucket = "none"
          }
        }

        for (i = 1; i <= length(line); i++) {
          ch = substr(line, i, 1)
          if (ch == "{") {
            depth += 1
            parent_cfg = (depth > 1 ? cfg_depth[depth - 1] : 0)
            parent_inline = (depth > 1 ? inline_depth[depth - 1] : 0)

            cfg_depth[depth] = (pending_cfg_test == 1 || parent_cfg == 1) ? 1 : 0
            inline_depth[depth] = (pending_inline_test == 1 || parent_inline == 1) ? 1 : 0

            if (pending_cfg_test == 1) {
              pending_cfg_test = 0
            }
            if (pending_inline_test == 1) {
              pending_inline_test = 0
            }
          } else if (ch == "}") {
            if (depth > 0) {
              delete cfg_depth[depth]
              delete inline_depth[depth]
              depth -= 1
            }
          }
        }

        if (pending_cfg_test == 1 && line ~ /;[[:space:]]*$/) {
          pending_cfg_test = 0
        }
        if (pending_inline_test == 1 && line ~ /;[[:space:]]*$/) {
          pending_inline_test = 0
        }

        if (NR == target_line && line_bucket == "none" && line ~ /panic!\(|unsafe[[:space:]]*\{|unsafe[[:space:]]+fn|unsafe[[:space:]]+impl|unsafe[[:space:]]+trait|unsafe[[:space:]]+extern/) {
          post_cfg = 0
          post_inline = 0
          for (d = 1; d <= depth; d++) {
            if (cfg_depth[d] == 1) {
              post_cfg = 1
            }
            if (inline_depth[d] == 1) {
              post_inline = 1
            }
          }
          if (post_cfg == 1) {
            line_bucket = "cfg_test_module"
          } else if (post_inline == 1) {
            line_bucket = "inline_test"
          }
        }
      }

      END {
        print line_bucket
      }
    ' "${absolute_path}"
  )"

  TEST_CONTEXT_CACHE["${cache_key}"]="${result}"
  printf '%s' "${result}"
}

classify_occurrence() {
  local file="$1"
  local line="$2"

  if is_test_path "${file}"; then
    printf 'path_test'
    return 0
  fi

  local test_bucket
  test_bucket="$(test_context_bucket_for_line "${file}" "${line}")"
  if [[ "${test_bucket}" != "none" ]]; then
    printf '%s' "${test_bucket}"
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
