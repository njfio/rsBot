# Spec: Issue #2379 - Stabilize Deterministic Mutation Lane for Session Cost Conformance

Status: Accepted

## Problem Statement
The broad `cargo-mutants --in-diff` workflow for the session usage/cost slice is not deterministic in this workspace: baseline selection includes unrelated tests and mutation runs can stall without a stable scoped command recipe. We need a canonical, scoped mutation lane that is reproducible for the C-01..C-04 session-cost conformance surface.

## Acceptance Criteria

### AC-1 Canonical session-cost mutation lane config is valid and scoped
Given the repository QA-loop config for session-cost mutation gating,
When the config is parsed and validated,
Then stage commands are schema-valid and scoped to the session-cost crates/files/tests only.

### AC-2 QA-loop execution fails fast on first failing stage for this lane
Given a QA-loop config where the first stage fails,
When the loop executes with zero retries,
Then later stages are not executed and the report root cause is the first stage.

### AC-3 Operator docs provide deterministic invocation contract
Given the mutation lane documentation,
When operators follow it,
Then they have canonical commands for diff generation, env overrides, and QA-loop execution.

## Scope

### In Scope
- Canonical QA-loop config for session-cost mutation lane.
- Deterministic command scoping for baseline + mutation stages.
- Conformance tests for config validity/scope and fail-fast behavior.
- Operator-facing doc for canonical invocation.

### Out of Scope
- General workspace-wide mutation policy.
- CI pipeline wiring changes.
- Non-session-cost mutation coverage expansion.

## Conformance Cases

| Case | AC | Tier | Input | Expected |
|---|---|---|---|---|
| C-01 | AC-1 | Integration | Load `docs/qa/session-cost-mutation.qa-loop.json` | Config validates; stage commands are scoped and include deterministic mutation flags |
| C-02 | AC-2 | Functional | QA-loop config with first-stage failure and second-stage side effect | Outcome is fail; second stage not executed; root cause is first stage |
| C-03 | AC-3 | Unit/Functional | Load `docs/qa/session-cost-mutation-lane.md` | Doc contains canonical diff/env/qa-loop invocation commands |

## Success Metrics / Observable Signals
- C-01..C-03 tests pass in `tau-ops`.
- Canonical lane can be invoked with documented command without broad workspace mutation scope.
