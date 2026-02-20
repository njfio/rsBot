# Spec: Issue #2758 - Discord polling message history backfill (up to 100 before trigger) (G10)

Status: Implemented

## Problem Statement
Tau Discord polling currently fetches a fixed poll batch and does not explicitly perform first-run history backfill up to 100 messages before trigger. Operators need immediate context from recent channel history when enabling polling, while preserving cursor-based incremental behavior on subsequent cycles.

## Acceptance Criteria

### AC-1 First-run Discord polling backfills up to 100 recent messages
Given Discord polling mode is enabled with a configured ingress channel and no saved Discord cursor for that channel,
When the poll cycle runs,
Then the connector requests up to 100 recent messages and ingests eligible messages in chronological order.

### AC-2 Subsequent polling remains incremental and avoids history replay
Given Discord polling mode with an existing saved message cursor for a channel,
When the poll cycle runs,
Then only messages newer than the saved cursor are ingested and older history is not replayed.

### AC-3 Guild allowlist filtering remains enforced during backfill
Given Discord polling mode with guild allowlist IDs configured,
When first-run backfill fetches mixed guild messages,
Then only messages from allowlisted guild IDs are ingested.

### AC-4 Verification artifacts and parity checklist evidence are updated
Given implementation is complete,
When scoped quality gates and local live validation run,
Then tests pass and `tasks/spacebot-comparison.md` marks G10 message history backfill with issue evidence.

## Scope

### In Scope
- Discord polling request-limit selection for first-run backfill versus incremental polling.
- Conformance/regression tests for first-run and cursored behavior.
- Guild allowlist compatibility validation during backfill.
- Spacebot parity checklist evidence update.

### Out of Scope
- Discord Serenity runtime migration.
- Discord send/attachment/thread/reaction/typing implementations.
- Webhook mode behavior changes.

## Conformance Cases
- C-01 (functional): first-run/no-cursor polling requests `limit=100` and ingests up to 100 messages.
- C-02 (regression): subsequent polling with saved cursor requests standard incremental limit and ingests only newer messages.
- C-03 (functional): first-run backfill still enforces guild allowlist filters.
- C-04 (verify/live): fmt, clippy, targeted tests, and local live validation pass.
- C-05 (docs): G10 backfill checklist row is checked with `#2758` evidence.

## Success Metrics / Observable Signals
- First-run Discord polling captures recent context (up to 100 messages) without manual replay steps.
- Incremental polling continues to be cursor-based and idempotent.
- Backfill does not bypass existing guild permission filters.
