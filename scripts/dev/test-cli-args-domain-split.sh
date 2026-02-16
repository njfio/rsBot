#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

cli_args_file="crates/tau-cli/src/cli_args.rs"
tail_flags_file="crates/tau-cli/src/cli_args/runtime_tail_flags.rs"

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "${haystack}" != *"${needle}"* ]]; then
    echo "assertion failed (${label}): expected to find '${needle}'" >&2
    exit 1
  fi
}

cli_line_count="$(wc -l < "${cli_args_file}" | tr -d ' ')"
if (( cli_line_count >= 4000 )); then
  echo "assertion failed (line budget): expected ${cli_args_file} < 4000 lines, got ${cli_line_count}" >&2
  exit 1
fi

if [[ ! -f "${tail_flags_file}" ]]; then
  echo "assertion failed (domain extraction file): missing ${tail_flags_file}" >&2
  exit 1
fi

cli_contents="$(cat "${cli_args_file}")"
tail_contents="$(cat "${tail_flags_file}")"

assert_contains "${cli_contents}" "mod runtime_tail_flags;" "cli module marker"
assert_contains "${cli_contents}" "pub runtime_tail: CliRuntimeTailFlags," "cli runtime tail flatten marker"
assert_contains "${tail_contents}" "pub custom_command_contract_runner: bool," "tail custom command marker"
assert_contains "${tail_contents}" "pub voice_contract_runner: bool," "tail voice marker"
assert_contains "${tail_contents}" "pub github_issues_bridge: bool," "tail github marker"
assert_contains "${tail_contents}" "pub struct CliRuntimeTailFlags {" "tail struct marker"
assert_contains "${tail_contents}" "pub gateway_daemon: CliGatewayDaemonFlags," "tail gateway flatten marker"

echo "cli-args-domain-split tests passed"
