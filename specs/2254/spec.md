# Spec #2254

Status: Implemented
Milestone: specs/milestones/m46/index.md
Issue: https://github.com/njfio/Tau/issues/2254

## Problem Statement

Runtime usage and spend signals are emitted during agent execution but are not
summed and persisted at the session level. Operators cannot answer "how many
tokens and dollars did this session consume?" via session commands.

## Scope

In scope:

- Persist cumulative per-session usage/cost totals for interactive runtime
  sessions.
- Update `/session-stats` outputs (text and json) to include usage/cost totals.
- Ensure totals survive process restart (store reload).

Out of scope:

- Provider billing reconciliation beyond existing estimated-cost math.
- Historical backfill for legacy sessions with no usage ledger.

## Acceptance Criteria

- AC-1: Successful prompt runs record usage/cost deltas into session-backed
  storage keyed by session path.
- AC-2: `/session-stats` text and json outputs include per-session
  `input_tokens`, `output_tokens`, `total_tokens`, and `estimated_cost_usd`.
- AC-3: Session usage/cost totals persist across reloads and are included in
  computed stats after restart.

## Conformance Cases

- C-01 (AC-1, functional): run prompt with a priced mock response and verify
  session usage/cost totals increase by expected deltas.
- C-02 (AC-2, conformance): `/session-stats` text contains usage/cost line and
  json payload contains corresponding fields.
- C-03 (AC-3, integration): reload session store/runtime and verify previously
  recorded totals remain intact.

## Success Metrics / Observable Signals

- `run_prompt_with_cancellation` writes a usage delta when status is
  `Completed`.
- `compute_session_stats` includes persisted usage totals in returned
  `SessionStats`.
- Targeted tests for `tau-session` and `tau-coding-agent` pass for all
  conformance cases.
