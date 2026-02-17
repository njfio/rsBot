# Spec #2357

Status: Accepted
Milestone: specs/milestones/m58/index.md
Issue: https://github.com/njfio/Tau/issues/2357

## Problem Statement

`tasks/spacebot-comparison.md` identifies Gap G11: Tau processes rapid inbound
channel messages independently, so split-thought user input can trigger
premature responses before the full request arrives.

## Scope

In scope:

- Add configurable coalescing window behavior in multi-channel runtime queueing.
- Batch compatible inbound events into one processed turn.
- Preserve ordering and deterministic duplicate suppression.
- Add unit/functional/integration/regression tests plus live runner validation.

Out of scope:

- Cross-runtime coalescing beyond `tau-multi-channel`.
- Provider/model-side prompt optimization.
- Changes to Slack/GitHub runtime loops outside multi-channel runtime contract.

## Acceptance Criteria

- AC-1: Given multiple inbound events with the same transport, conversation, and
  actor arriving within a configured coalescing window, when runtime processes
  a cycle, then they are batched into one processed event and user text is
  joined in timestamp order with newline separators.
- AC-2: Given events outside the coalescing window or with different
  transport/conversation/actor keys, when runtime processes a cycle, then they
  remain independent events.
- AC-3: Given a coalesced batch, when runtime records dedupe state, then all
  source event keys from the batch are marked processed.
- AC-4: Given CLI/runtime config, when coalescing window is set to `0`, then
  runtime disables batching and preserves pre-existing per-event behavior.
- AC-5: Given live ingress NDJSON with rapid same-conversation messages, when
  running the live runner path, then channel logs show one inbound context turn
  and one outbound response for that coalesced batch.

## Conformance Cases

- C-01 (AC-1, unit): coalescer merges two same-conversation events inside window.
- C-02 (AC-2, unit): coalescer does not merge events with different actor or
  outside window.
- C-03 (AC-3, functional): runtime marks every source event key as processed for
  one coalesced batch.
- C-04 (AC-4, regression): zero window keeps one-response-per-event behavior.
- C-05 (AC-5, integration/live): live runner fixture with two rapid events logs
  one outbound response and one user context turn.
