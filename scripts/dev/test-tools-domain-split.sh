#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

tools_file="crates/tau-tools/src/tools.rs"
bash_module_file="crates/tau-tools/src/tools/bash_tool.rs"

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "${haystack}" != *"${needle}"* ]]; then
    echo "assertion failed (${label}): expected to find '${needle}'" >&2
    exit 1
  fi
}

tools_line_count="$(wc -l < "${tools_file}" | tr -d ' ')"
if (( tools_line_count >= 3000 )); then
  echo "assertion failed (line budget): expected ${tools_file} < 3000 lines, got ${tools_line_count}" >&2
  exit 1
fi

if [[ ! -f "${bash_module_file}" ]]; then
  echo "assertion failed (domain extraction file): missing ${bash_module_file}" >&2
  exit 1
fi

tools_contents="$(cat "${tools_file}")"
bash_contents="$(cat "${bash_module_file}")"

assert_contains "${tools_contents}" "mod bash_tool;" "tools module marker"
assert_contains "${tools_contents}" "pub use bash_tool::BashTool;" "tools re-export marker"
assert_contains "${tools_contents}" "#[cfg(test)]" "tools cfg-test marker"
assert_contains "${tools_contents}" "mod tests;" "tools tests module marker"

assert_contains "${bash_contents}" "pub struct BashTool {" "bash module struct marker"
assert_contains "${bash_contents}" "impl AgentTool for BashTool {" "bash module impl marker"
assert_contains "${bash_contents}" "fn evaluate_tool_rate_limit_gate(" "bash module policy helper marker"

echo "tools-domain-split tests passed"
