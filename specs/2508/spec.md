# Spec #2508 - Validate multi-channel/slack coalescing against G11 checklist

Status: Implemented

## Problem Statement
We need end-to-end verification that G11 checklist behavior is actually met in active channel runtimes.

## Acceptance Criteria
### AC-1 Coalescing checklist behavior is explicitly verified
Given rapid same-conversation messages, when runtime runs, then batching, configurable windows, and newline dispatch are verifiable through tests.

### AC-2 Typing lifecycle signaling is present for coalesced processing
Given coalesced batches, when runtime processes outbound responses, then typing lifecycle telemetry/signaling is emitted deterministically.

## Scope
In scope:
- `tau-multi-channel` and `tau-slack-runtime` G11 validation/hardening.

Out of scope:
- New protocol integrations.

## Conformance Cases
- C-01 (AC-1, functional): coalescing batch formation + window behavior.
- C-02 (AC-2, functional/regression): coalesced processing emits typing lifecycle signal.

## Success Metrics
- Story conformance cases pass in scoped test runs.
