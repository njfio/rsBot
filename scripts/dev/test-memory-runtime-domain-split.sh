#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

runtime_file="crates/tau-memory/src/runtime.rs"
runtime_dir="crates/tau-memory/src/runtime"

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "${haystack}" != *"${needle}"* ]]; then
    echo "assertion failed (${label}): expected to find '${needle}'" >&2
    exit 1
  fi
}

assert_not_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "${haystack}" == *"${needle}"* ]]; then
    echo "assertion failed (${label}): expected to NOT find '${needle}'" >&2
    exit 1
  fi
}

line_count="$(wc -l < "${runtime_file}" | tr -d ' ')"
if (( line_count >= 2200 )); then
  echo "assertion failed (line budget): expected ${runtime_file} < 2200 lines, got ${line_count}" >&2
  exit 1
fi

for file in \
  "${runtime_dir}/backend.rs" \
  "${runtime_dir}/query.rs" \
  "${runtime_dir}/ranking.rs"; do
  if [[ ! -f "${file}" ]]; then
    echo "assertion failed (module file): missing ${file}" >&2
    exit 1
  fi
done

runtime_contents="$(cat "${runtime_file}")"

assert_contains "${runtime_contents}" "mod backend;" "module marker: backend"
assert_contains "${runtime_contents}" "mod query;" "module marker: query"
assert_contains "${runtime_contents}" "mod ranking;" "module marker: ranking"

assert_not_contains "${runtime_contents}" "fn resolve_memory_backend(" "moved helper: backend"
assert_not_contains "${runtime_contents}" "pub fn rank_text_candidates(" "moved helper: ranking"
assert_not_contains "${runtime_contents}" "pub fn search(&self, query: &str, options: &MemorySearchOptions)" "moved helper: query"

echo "memory-runtime-domain-split tests passed"
