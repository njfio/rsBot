#!/usr/bin/env bash
set -euo pipefail

scan_root="${1:-crates}"

if ! command -v rg >/dev/null 2>&1; then
  echo "error: ripgrep (rg) is required" >&2
  exit 1
fi

if [ ! -d "${scan_root}" ]; then
  echo "error: scan root does not exist: ${scan_root}" >&2
  exit 1
fi

is_test_path() {
  local path="$1"
  [[ "${path}" == *"/tests/"* ]] || [[ "${path}" == *"/src/tests/"* ]] || [[ "${path}" == *"tests.rs" ]] || [[ "${path}" == *"_test.rs" ]]
}

declare -A TEST_CONTEXT_CACHE=()

is_test_context_line() {
  local path="$1"
  local line_number="$2"
  local cache_key="${path}:${line_number}"

  if [[ -n "${TEST_CONTEXT_CACHE[${cache_key}]+x}" ]]; then
    [[ "${TEST_CONTEXT_CACHE[${cache_key}]}" == "1" ]]
    return
  fi

  if [[ ! -f "${path}" ]]; then
    TEST_CONTEXT_CACHE["${cache_key}"]="0"
    return 1
  fi

  local result
  result="$(
    awk -v target_line="${line_number}" '
      function has_test_attribute(text) {
        return text ~ /#\[cfg\([^]]*test[^]]*\)\]/ || text ~ /#\[[^]]*test[^]]*\]/
      }

      BEGIN {
        depth = 0
        pending_test = 0
        line_is_test = 0
      }

      {
        line = $0

        if (line !~ /#\[cfg_attr\(/ && has_test_attribute(line)) {
          pending_test = 1
        }

        current_test = 0
        for (d = 1; d <= depth; d++) {
          if (test_depth[d] == 1) {
            current_test = 1
            break
          }
        }

        if (NR == target_line) {
          line_is_test = current_test
        }

        for (i = 1; i <= length(line); i++) {
          ch = substr(line, i, 1)
          if (ch == "{") {
            depth += 1
            parent_test = (depth > 1 ? test_depth[depth - 1] : 0)
            if (pending_test == 1) {
              test_depth[depth] = 1
              pending_test = 0
            } else {
              test_depth[depth] = parent_test
            }
          } else if (ch == "}") {
            if (depth > 0) {
              delete test_depth[depth]
              depth -= 1
            }
          }
        }

        if (pending_test == 1 && line ~ /;[[:space:]]*$/) {
          pending_test = 0
        }

        if (NR == target_line && line_is_test == 0) {
          post_test = 0
          for (d = 1; d <= depth; d++) {
            if (test_depth[d] == 1) {
              post_test = 1
              break
            }
          }
          if (line ~ /panic!\(|unsafe[[:space:]]*\{|unsafe[[:space:]]+fn/) {
            line_is_test = post_test
          }
        }
      }

      END {
        if (line_is_test == 1) {
          print 1
        } else {
          print 0
        }
      }
    ' "${path}"
  )"

  TEST_CONTEXT_CACHE["${cache_key}"]="${result}"
  [[ "${result}" == "1" ]]
}

print_group() {
  local title="$1"
  shift
  echo "${title}:"
  if [ "$#" -eq 0 ]; then
    echo "  (none)"
    return
  fi
  for entry in "$@"; do
    echo "  ${entry}"
  done
}

summarize_matches() {
  local label="$1"
  local matches="$2"
  local cleaned
  cleaned="$(printf '%s\n' "${matches}" | sed '/^$/d' | sort || true)"

  local total=0
  local test_count=0
  local non_test_count=0
  local test_lines=()
  local non_test_lines=()

  while IFS= read -r line; do
    [ -z "${line}" ] && continue
    total=$((total + 1))
    local path="${line%%:*}"
    local remainder="${line#*:}"
    local line_number="${remainder%%:*}"
    if is_test_path "${path}" || is_test_context_line "${path}" "${line_number}"; then
      test_count=$((test_count + 1))
      test_lines+=("${line}")
    else
      non_test_count=$((non_test_count + 1))
      non_test_lines+=("${line}")
    fi
  done <<< "${cleaned}"

  echo "${label}_total=${total}"
  echo "${label}_test_path=${test_count}"
  echo "${label}_non_test_path=${non_test_count}"
  print_group "${label}_test_matches" "${test_lines[@]}"
  print_group "${label}_non_test_matches" "${non_test_lines[@]}"
}

panic_matches="$(rg -n --no-heading --glob '*.rs' 'panic!\(' "${scan_root}" || true)"
unsafe_matches="$(rg -n --no-heading --glob '*.rs' '\bunsafe\s*\{|\bunsafe\s+fn\b' "${scan_root}" || true)"

echo "panic_unsafe_audit_root=${scan_root}"
summarize_matches "panic" "${panic_matches}"
summarize_matches "unsafe" "${unsafe_matches}"
