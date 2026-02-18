# Spec #2509 - Implement/validate G11 conformance tests and runtime wiring

Status: Implemented

## Problem Statement
G11 checklist closure requires explicit conformance tests proving coalescing behavior and typing lifecycle signaling for coalesced batches.

## Acceptance Criteria
### AC-1 Coalescing merges rapid same-conversation messages
Given same-actor/same-conversation events inside the coalescing window, when coalescer runs, then one batch is emitted and text is newline-joined.

### AC-2 Coalescing window remains configurable
Given window set to zero, when runtime runs, then events are not coalesced and each source event is processed independently.

### AC-3 Coalesced batches force typing lifecycle signaling
Given coalesced batch size > 1, when runtime processes the coalesced event, then typing lifecycle telemetry signaling is emitted even for short replies.

### AC-4 Live ingestion path preserves single-turn coalesced response
Given live ingress rapid messages in one conversation, when live runner executes, then a single user turn and a single outbound response are produced.

## Scope
In scope:
- `tau-multi-channel` coalescing metadata + typing lifecycle logic.
- G11 conformance tests and checklist update.

Out of scope:
- Slack API changes or new transport protocols.

## Conformance Cases
- C-01 (AC-1, functional): `spec_2509_c01_coalescer_merges_and_newline_joins_within_window`
- C-02 (AC-2, regression): `regression_spec_2509_c02_zero_window_keeps_per_event_processing`
- C-03 (AC-3, integration): `integration_spec_2509_c03_coalesced_batch_forces_typing_lifecycle_signals`
- C-04 (AC-4, integration/live): `integration_spec_2509_c04_live_runner_coalesces_to_single_turn`

## Success Metrics
- C-01..C-04 pass.
- G11 checklist items marked complete.
