#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

runtime_file="crates/tau-github-issues-runtime/src/github_issues_runtime.rs"
rendering_module_file="crates/tau-github-issues-runtime/src/github_issues_runtime/issue_command_rendering.rs"

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "${haystack}" != *"${needle}"* ]]; then
    echo "assertion failed (${label}): expected to find '${needle}'" >&2
    exit 1
  fi
}

runtime_line_count="$(wc -l < "${runtime_file}" | tr -d ' ')"
if (( runtime_line_count >= 3000 )); then
  echo "assertion failed (line budget): expected ${runtime_file} < 3000 lines, got ${runtime_line_count}" >&2
  exit 1
fi

if [[ ! -f "${rendering_module_file}" ]]; then
  echo "assertion failed (domain extraction file): missing ${rendering_module_file}" >&2
  exit 1
fi

runtime_contents="$(cat "${runtime_file}")"
rendering_contents="$(cat "${rendering_module_file}")"

assert_contains "${runtime_contents}" "mod issue_command_rendering;" "runtime module marker"
assert_contains "${runtime_contents}" "#[cfg(test)]" "runtime cfg-test marker"
assert_contains "${runtime_contents}" "mod tests;" "runtime tests module marker"

assert_contains "${rendering_contents}" "impl GithubIssuesBridgeRuntime {" "rendering impl marker"
assert_contains "${rendering_contents}" "fn render_issue_status(&self, issue_number: u64) -> String {" "rendering status marker"
assert_contains "${rendering_contents}" "async fn post_issue_command_comment(" "rendering async marker"

echo "github-issues-runtime-domain-split tests passed"
