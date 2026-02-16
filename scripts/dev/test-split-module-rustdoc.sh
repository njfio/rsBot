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
issue_run_error_comment_file="crates/tau-github-issues/src/issue_run_error_comment.rs"
retry_file="crates/tau-ai/src/retry.rs"
slack_helpers_file="crates/tau-runtime/src/slack_helpers_runtime.rs"
events_cli_commands_file="crates/tau-events/src/events_cli_commands.rs"
deployment_wasm_runtime_file="crates/tau-deployment/src/deployment_wasm_runtime.rs"
rpc_capabilities_file="crates/tau-runtime/src/rpc_capabilities_runtime.rs"
rpc_transport_file="crates/tau-runtime/src/rpc_protocol_runtime/transport.rs"
rpc_dispatch_file="crates/tau-runtime/src/rpc_protocol_runtime/dispatch.rs"
rpc_parsing_file="crates/tau-runtime/src/rpc_protocol_runtime/parsing.rs"
runtime_output_file="crates/tau-runtime/src/runtime_output_runtime.rs"
startup_model_catalog_file="crates/tau-startup/src/startup_model_catalog.rs"
startup_multi_channel_adapters_file="crates/tau-startup/src/startup_multi_channel_adapters.rs"
startup_multi_channel_commands_file="crates/tau-startup/src/startup_multi_channel_commands.rs"
startup_rpc_capabilities_command_file="crates/tau-startup/src/startup_rpc_capabilities_command.rs"
onboarding_command_file="crates/tau-onboarding/src/onboarding_command.rs"
onboarding_daemon_file="crates/tau-onboarding/src/onboarding_daemon.rs"
onboarding_paths_file="crates/tau-onboarding/src/onboarding_paths.rs"
onboarding_profile_bootstrap_file="crates/tau-onboarding/src/onboarding_profile_bootstrap.rs"
startup_daemon_preflight_file="crates/tau-onboarding/src/startup_daemon_preflight.rs"
startup_resolution_file="crates/tau-onboarding/src/startup_resolution.rs"
tool_policy_config_file="crates/tau-tools/src/tool_policy_config.rs"
tools_runtime_helpers_file="crates/tau-tools/src/tools/runtime_helpers.rs"

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
  "${issue_run_error_comment_file}" \
  "${retry_file}" \
  "${slack_helpers_file}" \
  "${events_cli_commands_file}" \
  "${deployment_wasm_runtime_file}" \
  "${rpc_capabilities_file}" \
  "${rpc_transport_file}" \
  "${rpc_dispatch_file}" \
  "${rpc_parsing_file}" \
  "${runtime_output_file}" \
  "${startup_model_catalog_file}" \
  "${startup_multi_channel_adapters_file}" \
  "${startup_multi_channel_commands_file}" \
  "${startup_rpc_capabilities_command_file}" \
  "${onboarding_command_file}" \
  "${onboarding_daemon_file}" \
  "${onboarding_paths_file}" \
  "${onboarding_profile_bootstrap_file}" \
  "${startup_daemon_preflight_file}" \
  "${startup_resolution_file}" \
  "${tool_policy_config_file}" \
  "${tools_runtime_helpers_file}"; do
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
issue_run_error_comment_contents="$(cat "${issue_run_error_comment_file}")"
retry_contents="$(cat "${retry_file}")"
slack_contents="$(cat "${slack_helpers_file}")"
events_cli_contents="$(cat "${events_cli_commands_file}")"
deployment_wasm_runtime_contents="$(cat "${deployment_wasm_runtime_file}")"
rpc_capabilities_contents="$(cat "${rpc_capabilities_file}")"
rpc_transport_contents="$(cat "${rpc_transport_file}")"
rpc_dispatch_contents="$(cat "${rpc_dispatch_file}")"
rpc_parsing_contents="$(cat "${rpc_parsing_file}")"
runtime_output_contents="$(cat "${runtime_output_file}")"
startup_model_catalog_contents="$(cat "${startup_model_catalog_file}")"
startup_multi_channel_adapters_contents="$(cat "${startup_multi_channel_adapters_file}")"
startup_multi_channel_commands_contents="$(cat "${startup_multi_channel_commands_file}")"
startup_rpc_capabilities_command_contents="$(cat "${startup_rpc_capabilities_command_file}")"
onboarding_command_contents="$(cat "${onboarding_command_file}")"
onboarding_daemon_contents="$(cat "${onboarding_daemon_file}")"
onboarding_paths_contents="$(cat "${onboarding_paths_file}")"
onboarding_profile_bootstrap_contents="$(cat "${onboarding_profile_bootstrap_file}")"
startup_daemon_preflight_contents="$(cat "${startup_daemon_preflight_file}")"
startup_resolution_contents="$(cat "${startup_resolution_file}")"
tool_policy_config_contents="$(cat "${tool_policy_config_file}")"
tools_runtime_helpers_contents="$(cat "${tools_runtime_helpers_file}")"

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
assert_contains "${issue_run_error_comment_contents}" "/// Render GitHub issue comment body for failed Tau run events." "issue run error comment doc"
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
assert_contains "${rpc_dispatch_contents}" "/// Dispatch one validated RPC frame into a response envelope." "rpc dispatch frame doc"
assert_contains "${rpc_parsing_contents}" "/// Parse one raw JSON RPC frame string into validated runtime frame." "rpc parsing frame doc"
assert_contains "${rpc_parsing_contents}" "/// Load and validate one RPC frame JSON file from disk." "rpc parsing file doc"
assert_contains "${runtime_output_contents}" "/// Summarize one chat message for compact operator-facing logs." "runtime output summarize doc"
assert_contains "${runtime_output_contents}" "/// Convert one agent event into deterministic JSON for logs and snapshots." "runtime output event json doc"
assert_contains "${startup_model_catalog_contents}" "/// Resolve startup model catalog using CLI cache/refresh settings." "startup model catalog resolve doc"
assert_contains "${startup_model_catalog_contents}" "/// Validate startup primary and fallback models support required tool calling." "startup model catalog validate doc"
assert_contains "${startup_multi_channel_adapters_contents}" "/// Build multi-channel command handlers backed by startup auth/doctor configs." "startup adapters handlers doc"
assert_contains "${startup_multi_channel_adapters_contents}" "/// Build pairing evaluator adapter for multi-channel policy checks." "startup adapters pairing evaluator doc"
assert_contains "${startup_multi_channel_commands_contents}" "/// Execute multi-channel send command from CLI runtime configuration." "startup multi-channel send doc"
assert_contains "${startup_multi_channel_commands_contents}" "/// Execute multi-channel channel lifecycle command for login/logout/status/probe." "startup multi-channel lifecycle doc"
assert_contains "${startup_rpc_capabilities_command_contents}" "/// Execute RPC capabilities command and print negotiated payload when requested." "startup rpc capabilities command doc"
assert_contains "${onboarding_command_contents}" "/// Execute onboarding command and persist onboarding summary report." "onboarding command execute doc"
assert_contains "${onboarding_daemon_contents}" "/// Run onboarding daemon bootstrap flow and return readiness report." "onboarding daemon bootstrap doc"
assert_contains "${onboarding_paths_contents}" "/// Resolve Tau root directory used for onboarding bootstrap artifacts." "onboarding paths resolve root doc"
assert_contains "${onboarding_paths_contents}" "/// Collect deduplicated onboarding bootstrap directories derived from CLI paths." "onboarding paths collect directories doc"
assert_contains "${onboarding_profile_bootstrap_contents}" "/// Resolve onboarding profile name, applying default when input is blank." "onboarding profile resolve name doc"
assert_contains "${onboarding_profile_bootstrap_contents}" "/// Ensure onboarding profile store contains requested profile defaults." "onboarding profile ensure store doc"
assert_contains "${startup_daemon_preflight_contents}" "/// Handle daemon CLI commands during startup preflight and short-circuit when executed." "startup daemon preflight handle doc"
assert_contains "${startup_resolution_contents}" "/// Resolve startup system prompt, optionally loading content from file path." "startup resolution system prompt doc"
assert_contains "${startup_resolution_contents}" "/// Ensure resolved startup text is non-empty after trimming whitespace." "startup resolution non-empty doc"
assert_contains "${startup_resolution_contents}" "/// Resolve trusted skill roots from inline flags and optional trust-root store." "startup resolution trust roots doc"
assert_contains "${startup_resolution_contents}" "/// Apply trust-root add/revoke/rotate mutations against in-memory records." "startup resolution trust mutation doc"
assert_contains "${tool_policy_config_contents}" "/// Build runtime tool policy from CLI arguments and environment overrides." "tool policy config build doc"
assert_contains "${tool_policy_config_contents}" "/// Parse --os-sandbox-command values into normalized command tokens." "tool policy config parse sandbox tokens doc"
assert_contains "${tool_policy_config_contents}" "/// Convert tool policy into JSON payload for diagnostics and audit output." "tool policy config json doc"
assert_contains "${tools_runtime_helpers_contents}" "/// Return stable string label for OS sandbox policy mode." "tools runtime helpers policy mode name doc"
assert_contains "${tools_runtime_helpers_contents}" "/// Return stable string label for OS sandbox docker network mode." "tools runtime helpers docker network name doc"

echo "split-module-rustdoc tests passed"
