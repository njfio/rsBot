#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

issue_runtime_helpers="crates/tau-github-issues/src/issue_runtime_helpers.rs"
issue_command_usage="crates/tau-github-issues/src/issue_command_usage.rs"
github_transport_helpers="crates/tau-github-issues/src/github_transport_helpers.rs"
issue_filter_file="crates/tau-github-issues/src/issue_filter.rs"
issue_session_helpers_file="crates/tau-github-issues/src/issue_session_helpers.rs"
issue_prompt_helpers_file="crates/tau-github-issues/src/issue_prompt_helpers.rs"
retry_file="crates/tau-ai/src/retry.rs"
slack_helpers_file="crates/tau-runtime/src/slack_helpers_runtime.rs"
events_cli_commands_file="crates/tau-events/src/events_cli_commands.rs"
deployment_wasm_runtime_file="crates/tau-deployment/src/deployment_wasm_runtime.rs"
rpc_capabilities_file="crates/tau-runtime/src/rpc_capabilities_runtime.rs"
rpc_transport_file="crates/tau-runtime/src/rpc_protocol_runtime/transport.rs"

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "${haystack}" != *"${needle}"* ]]; then
    echo "assertion failed (${label}): expected to find '${needle}'" >&2
    exit 1
  fi
}

for file in \
  "${issue_runtime_helpers}" \
  "${issue_command_usage}" \
  "${github_transport_helpers}" \
  "${issue_filter_file}" \
  "${issue_session_helpers_file}" \
  "${issue_prompt_helpers_file}" \
  "${retry_file}" \
  "${slack_helpers_file}" \
  "${events_cli_commands_file}" \
  "${deployment_wasm_runtime_file}" \
  "${rpc_capabilities_file}" \
  "${rpc_transport_file}"; do
  if [[ ! -f "${file}" ]]; then
    echo "assertion failed (missing file): ${file}" >&2
    exit 1
  fi
done

issue_runtime_contents="$(cat "${issue_runtime_helpers}")"
issue_usage_contents="$(cat "${issue_command_usage}")"
github_transport_contents="$(cat "${github_transport_helpers}")"
issue_filter_contents="$(cat "${issue_filter_file}")"
issue_session_helpers_contents="$(cat "${issue_session_helpers_file}")"
issue_prompt_helpers_contents="$(cat "${issue_prompt_helpers_file}")"
retry_contents="$(cat "${retry_file}")"
slack_contents="$(cat "${slack_helpers_file}")"
events_cli_contents="$(cat "${events_cli_commands_file}")"
deployment_wasm_runtime_contents="$(cat "${deployment_wasm_runtime_file}")"
rpc_capabilities_contents="$(cat "${rpc_capabilities_file}")"
rpc_transport_contents="$(cat "${rpc_transport_file}")"

assert_contains "${issue_runtime_contents}" "/// Normalize a repository-relative channel artifact path for persisted pointers." "issue runtime normalize doc"
assert_contains "${issue_runtime_contents}" "/// Render a stable artifact pointer line for issue comments and logs." "issue runtime pointer doc"
assert_contains "${issue_usage_contents}" "/// Render doctor command usage for GitHub issue transport runtime commands." "issue usage doctor doc"
assert_contains "${issue_usage_contents}" "/// Render top-level /tau help lines scoped to issue-transport commands." "issue usage root doc"
assert_contains "${github_transport_contents}" "/// Parse GitHub Retry-After header value as a duration." "github transport retry-after doc"
assert_contains "${github_transport_contents}" "/// Determine whether a GitHub HTTP status is retryable." "github transport retry-status doc"
assert_contains "${issue_filter_contents}" "/// Normalize issue label filters for case-insensitive matching." "issue filter normalize doc"
assert_contains "${issue_filter_contents}" "/// Return true when issue labels satisfy required label filters." "issue filter label-match doc"
assert_contains "${issue_session_helpers_contents}" "/// Compact one issue session store to active lineage while honoring lock policy." "issue session compact doc"
assert_contains "${issue_session_helpers_contents}" "/// Ensure issue session store exists and is initialized with system prompt." "issue session ensure doc"
assert_contains "${issue_prompt_helpers_contents}" "/// Collect non-empty assistant reply text from message history." "issue prompt collect doc"
assert_contains "${issue_prompt_helpers_contents}" "/// Build summarize prompt for one issue thread with optional focus." "issue prompt summarize doc"
assert_contains "${retry_contents}" "/// Default retry backoff base in milliseconds for provider HTTP requests." "retry base const doc"
assert_contains "${retry_contents}" "/// Return true when an HTTP status should be retried by provider clients." "retry status doc"
assert_contains "${slack_contents}" "/// Parse Slack Retry-After header value (seconds) when present." "slack retry-after doc"
assert_contains "${slack_contents}" "/// Build a deterministic path-safe slug for Slack channel identifiers." "slack sanitize doc"
assert_contains "${events_cli_contents}" "/// Execute events inspect mode and print either JSON or text report." "events inspect command doc"
assert_contains "${events_cli_contents}" "/// Execute events dry-run mode and enforce configured gate thresholds." "events dry-run command doc"
assert_contains "${deployment_wasm_runtime_contents}" "/// Execute WASM package command when --deployment-wasm-package-module is set." "deployment wasm package command doc"
assert_contains "${deployment_wasm_runtime_contents}" "/// Execute browser DID initialization command for deployment WASM runtime." "deployment wasm did command doc"
assert_contains "${rpc_capabilities_contents}" "/// Build canonical RPC capabilities payload for CLI and NDJSON protocol clients." "rpc capabilities payload doc"
assert_contains "${rpc_capabilities_contents}" "/// Render RPC capabilities payload as pretty JSON text." "rpc capabilities render doc"
assert_contains "${rpc_transport_contents}" "/// Dispatch one raw RPC frame string and always return a response envelope." "rpc transport dispatch doc"
assert_contains "${rpc_transport_contents}" "/// Serve NDJSON RPC frames from a reader and stream responses to writer." "rpc transport serve doc"

echo "split-module-rustdoc tests passed"
