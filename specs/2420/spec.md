# Spec: Issue #2420 - Slack bridge message coalescing

Status: Accepted

## Problem Statement
Slack bridge currently starts a run for the first queued event immediately. When users split one thought across several quick messages, Tau responds too early and loses context cohesion. G11 requires batching rapid-fire messages into one agent turn.

## Acceptance Criteria

### AC-1 Coalescing window delays run start for fresh queued events
Given an idle channel with a queued Slack event,
When coalescing window is enabled and the first event age is below the window,
Then runtime does not start a channel run yet.

### AC-2 Rapid events from same user/thread are coalesced into one run
Given multiple queued events in a channel from the same user and same reply thread within the coalescing window,
When runtime dequeues work for a run,
Then events are batched into a single run prompt joined by newline boundaries.

### AC-3 Non-coalescible events remain separate
Given queued events from different users or different reply threads or outside the coalescing gap,
When runtime dequeues work,
Then only the eligible contiguous coalescing segment is batched and remaining events stay queued for later runs.

### AC-4 Transport config wires coalescing window from CLI to runtime
Given Slack bridge startup configuration,
When CLI/onboarding/runtime config is built,
Then coalescing window field is present with default value 2000ms and supports explicit override.

## Scope

### In Scope
- Slack runtime queue coalescing logic and helper functions.
- CLI/onboarding/runtime config propagation for coalescing window.
- Conformance tests in slack runtime and onboarding transport config.

### Out of Scope
- Multi-transport or gateway-level coalescing.
- New dependencies.

## Conformance Cases
- C-01 (AC-1, functional): `spec_c01_try_start_queued_runs_respects_coalescing_window_before_dispatch`
- C-02 (AC-2, functional): `spec_c02_dequeue_coalesced_run_batches_same_user_and_thread_messages`
- C-03 (AC-3, regression): `regression_spec_c03_dequeue_coalesced_run_preserves_non_coalescible_tail`
- C-04 (AC-4, integration): `spec_c04_build_slack_bridge_cli_config_wires_coalescing_window_defaults_and_overrides`

## Success Metrics / Observable Signals
- New conformance tests pass.
- Existing Slack runtime behavior tests remain green.
- `cargo fmt --check`, `cargo clippy -p tau-slack-runtime -- -D warnings`, and scoped test commands pass.
