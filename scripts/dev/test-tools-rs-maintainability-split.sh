#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

tools_rs="crates/tau-tools/src/tools.rs"
tools_dir="crates/tau-tools/src/tools"

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

tools_lines="$(wc -l <"${tools_rs}" | tr -d '[:space:]')"
if (( tools_lines >= 2500 )); then
  echo "assertion failed (line budget): expected ${tools_rs} < 2500 lines, got ${tools_lines}" >&2
  exit 1
fi

for file in \
  "${tools_dir}/memory_tools.rs" \
  "${tools_dir}/jobs_tools.rs"; do
  if [[ ! -f "${file}" ]]; then
    echo "assertion failed (module file): missing ${file}" >&2
    exit 1
  fi
done

tools_contents="$(cat "${tools_rs}")"
assert_contains "${tools_contents}" "mod memory_tools;" "module marker: memory"
assert_contains "${tools_contents}" "mod jobs_tools;" "module marker: jobs"
assert_contains "${tools_contents}" "pub use memory_tools::" "memory re-export"
assert_contains "${tools_contents}" "pub use jobs_tools::" "jobs re-export"

assert_not_contains "${tools_contents}" "pub struct MemoryWriteTool" "moved memory tool struct"
assert_not_contains "${tools_contents}" "pub struct JobsCreateTool" "moved jobs tool struct"

echo "tools-rs-maintainability-split tests passed"
