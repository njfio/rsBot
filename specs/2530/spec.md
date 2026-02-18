# Spec #2530 - Task: add G14 send-file adapter wiring + validation

Status: Reviewed

## Problem Statement
`SendFileTool` exists, but adapter wiring is incomplete: multi-channel outbound rejects Discord send-file and Slack runtime does not dispatch tool-driven file delivery.

## Acceptance Criteria
### AC-1 discord send-file delivery
Given `MultiChannelTransport::Discord`, when `deliver_file` is called with a valid file URL, then dry-run and provider mode produce a sent receipt with deterministic request metadata and provider message id extraction.

### AC-2 telegram send-file regression
Given `MultiChannelTransport::Telegram`, when `deliver_file` is called with valid inputs, then existing `sendDocument` behavior remains unchanged.

### AC-3 slack send-file directive dispatch
Given Slack runtime prompt messages include a successful `send_file` tool result, when run completes, then Slack runtime uploads the file via Slack v2 file API path and records send-file metadata in logs/artifacts.

### AC-4 deterministic failures
Given invalid file URL or unsupported transport, when send-file dispatch fails, then reason codes remain explicit and stable.

## Scope
In scope:
- Adapter dispatch behavior for Discord/Telegram in multi-channel outbound.
- Slack runtime directive extraction + upload dispatch.
- Conformance and regression tests.

Out of scope:
- New transport types.
- Non-send-file tool behavior changes.

## Conformance Cases
- C-01 (AC-1, functional): `spec_2530_c01_functional_dry_run_shapes_discord_send_file_payload`
- C-02 (AC-1, integration): `spec_2530_c02_integration_provider_mode_posts_discord_send_file_request`
- C-03 (AC-2, integration): `spec_2530_c03_integration_provider_mode_posts_telegram_send_file_request`
- C-04 (AC-3, integration): `spec_2530_c04_slack_send_file_directive_dispatches_file_upload`
- C-05 (AC-4, regression): `regression_2530_send_file_still_rejects_unsupported_transport`

## Success Metrics
- C-01..C-05 pass.
- Full workspace tests pass after integration.
- Diff-scoped mutation run reports zero missed mutants.
- Live validation script completes successfully.
