#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

lib_file="crates/tau-agent-core/src/lib.rs"

startup_file="crates/tau-agent-core/src/runtime_startup.rs"
turn_loop_file="crates/tau-agent-core/src/runtime_turn_loop.rs"
tool_bridge_file="crates/tau-agent-core/src/runtime_tool_bridge.rs"
safety_memory_file="crates/tau-agent-core/src/runtime_safety_memory.rs"

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

line_count="$(wc -l < "${lib_file}" | tr -d ' ')"
if (( line_count >= 2600 )); then
  echo "assertion failed (line budget): expected ${lib_file} < 2600 lines, got ${line_count}" >&2
  exit 1
fi

for file in "${startup_file}" "${turn_loop_file}" "${tool_bridge_file}" "${safety_memory_file}"; do
  if [[ ! -f "${file}" ]]; then
    echo "assertion failed (module file): missing ${file}" >&2
    exit 1
  fi
done

lib_contents="$(cat "${lib_file}")"

assert_contains "${lib_contents}" "mod runtime_startup;" "module marker: startup"
assert_contains "${lib_contents}" "mod runtime_turn_loop;" "module marker: turn loop"
assert_contains "${lib_contents}" "mod runtime_tool_bridge;" "module marker: tool bridge"
assert_contains "${lib_contents}" "mod runtime_safety_memory;" "module marker: safety/memory"

assert_not_contains "${lib_contents}" "fn retrieve_memory_matches(" "moved helper: safety/memory"
assert_not_contains "${lib_contents}" "fn execute_tool_call_inner(" "moved helper: tool bridge"
assert_not_contains "${lib_contents}" "fn normalize_direct_message_content(" "moved helper: startup"
assert_not_contains "${lib_contents}" "fn bounded_messages(" "moved helper: turn loop"

echo "agent-core-lib-domain-split tests passed"
