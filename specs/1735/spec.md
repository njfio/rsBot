# Issue 1735 Spec

Status: Implemented

Issue: `#1735`  
Milestone: `#24`  
Parent: `#1658`

## Problem Statement

The new span-to-trajectory adapter needs fidelity proof on realistic multi-turn
tool-using traces. Current tests verify basic mapping, but not explicit
tool-call traces and edge-case field gaps with assertion depth.

## Scope

In scope:

- add multi-turn tool-trace fixtures in adapter tests
- assert observation/action/reward mapping fidelity for each turn
- assert explicit fallback behavior for missing fields

Out of scope:

- runtime collector/store changes
- PPO/GAE training logic changes

## Acceptance Criteria

AC-1:
Given multi-turn tool-using span traces,
when adapted to trajectories,
then step ordering and turn mapping are deterministic.

AC-2:
Given tool-call and tool-result spans,
when adapted,
then state/action/reward fields preserve expected semantics.

AC-3:
Given missing observation/action fields,
when adapted,
then fallback behavior is explicit and test-asserted.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Integration | Given a 3-turn tool trace, when adapted, then step indexes are sequential and final step is terminal. |
| C-02 | AC-2 | Functional | Given tool-call and tool-result attributes, when adapted, then observation/action/reward values map as expected. |
| C-03 | AC-3 | Regression | Given spans with missing fields, when adapted, then fallback objects are emitted with deterministic keys. |

## Success Metrics

- adapter fidelity for tool traces is demonstrable via deterministic tests
