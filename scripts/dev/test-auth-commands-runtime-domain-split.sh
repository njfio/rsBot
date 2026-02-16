#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

runtime_file="crates/tau-provider/src/auth_commands_runtime.rs"
runtime_dir="crates/tau-provider/src/auth_commands_runtime"

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
if (( line_count >= 2400 )); then
  echo "assertion failed (line budget): expected ${runtime_file} < 2400 lines, got ${line_count}" >&2
  exit 1
fi

for file in \
  "${runtime_dir}/shared_runtime_core.rs" \
  "${runtime_dir}/google_backend.rs" \
  "${runtime_dir}/openai_backend.rs" \
  "${runtime_dir}/anthropic_backend.rs"; do
  if [[ ! -f "${file}" ]]; then
    echo "assertion failed (module file): missing ${file}" >&2
    exit 1
  fi
done

runtime_contents="$(cat "${runtime_file}")"

assert_contains "${runtime_contents}" "mod shared_runtime_core;" "module marker: shared core"
assert_contains "${runtime_contents}" "mod google_backend;" "module marker: google backend"
assert_contains "${runtime_contents}" "mod openai_backend;" "module marker: openai backend"
assert_contains "${runtime_contents}" "mod anthropic_backend;" "module marker: anthropic backend"

assert_not_contains "${runtime_contents}" "fn collect_non_empty_secrets(" "moved helper: shared core"
assert_not_contains "${runtime_contents}" "fn execute_google_login_backend_ready(" "moved helper: google backend"
assert_not_contains "${runtime_contents}" "fn execute_openai_login_backend_ready(" "moved helper: openai backend"
assert_not_contains "${runtime_contents}" "fn execute_anthropic_login_backend_ready(" "moved helper: anthropic backend"

echo "auth-commands-runtime-domain-split tests passed"
