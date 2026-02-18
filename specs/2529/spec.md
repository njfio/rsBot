# Spec #2529 - Story: adapter-level send-file dispatch paths

Status: Reviewed

## Problem Statement
Current send-file behavior is transport-incomplete: Telegram path exists, while Discord and Slack adapter dispatch paths are missing or not wired.

## Acceptance Criteria
### AC-1 discord + telegram multi-channel coverage
Given `/tau send-file` command flows in `tau-multi-channel`, when dispatch executes, then Telegram and Discord file-delivery paths produce structured success receipts.

### AC-2 slack runtime send-file behavior
Given Slack bridge runs with built-in tools, when `send_file` is requested by the model, then Slack runtime dispatches a file upload path and records structured outcome.

### AC-3 failure contracts remain explicit
Given invalid/unsupported send-file inputs, when dispatch fails, then reason codes remain deterministic and auditable.

## Scope
In scope:
- `crates/tau-multi-channel/src/multi_channel_outbound.rs`
- `crates/tau-slack-runtime/src/slack_runtime.rs`
- `crates/tau-slack-runtime/src/slack_runtime/slack_api_client.rs`
- related tests

Out of scope:
- Non-G14 platform features.

## Conformance Cases
- C-01 (AC-1, functional): `spec_2530_c01_functional_dry_run_shapes_discord_send_file_payload`
- C-02 (AC-1, integration): `spec_2530_c02_integration_provider_mode_posts_discord_send_file_request`
- C-03 (AC-2, integration): `spec_2530_c03_slack_send_file_directive_dispatches_file_upload`
- C-04 (AC-3, regression): `regression_2530_send_file_still_rejects_unsupported_transport`

## Success Metrics
- C-01..C-04 pass.
- No regression in existing reaction/text outbound behavior.
