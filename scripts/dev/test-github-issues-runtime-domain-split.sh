#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

runtime_file="crates/tau-github-issues-runtime/src/github_issues_runtime.rs"
runtime_dir="crates/tau-github-issues-runtime/src/github_issues_runtime"

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
if (( runtime_line_count >= 4000 )); then
  echo "assertion failed (line budget): expected ${runtime_file} < 4000 lines, got ${runtime_line_count}" >&2
  exit 1
fi

if [[ ! -d "${runtime_dir}" ]]; then
  echo "assertion failed (runtime module dir): missing ${runtime_dir}" >&2
  exit 1
fi

runtime_contents="$(cat "${runtime_file}")"
assert_contains "${runtime_contents}" "mod github_api_client;" "module marker: github_api_client"
assert_contains "${runtime_contents}" "mod issue_command_helpers;" "module marker: issue_command_helpers"
assert_contains "${runtime_contents}" "mod issue_render_helpers;" "module marker: issue_render_helpers"
assert_contains "${runtime_contents}" "mod issue_run_task;" "module marker: issue_run_task"
assert_contains "${runtime_contents}" "mod issue_state_store;" "module marker: issue_state_store"
assert_contains "${runtime_contents}" "mod issue_session_runtime;" "module marker: issue_session_runtime"
assert_contains "${runtime_contents}" "mod prompt_execution;" "module marker: prompt_execution"

for extracted_file in \
  "github_api_client.rs" \
  "issue_command_helpers.rs" \
  "issue_render_helpers.rs" \
  "issue_run_task.rs" \
  "issue_state_store.rs" \
  "issue_session_runtime.rs" \
  "prompt_execution.rs"; do
  if [[ ! -f "${runtime_dir}/${extracted_file}" ]]; then
    echo "assertion failed (domain extraction file): missing ${runtime_dir}/${extracted_file}" >&2
    exit 1
  fi
done

echo "github-issues-runtime-domain-split tests passed"
