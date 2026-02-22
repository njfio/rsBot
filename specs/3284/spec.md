# Spec: Issue #3284 - add composite live reward evaluator and span breakdown attributes

Status: Implemented

## Problem Statement
The live RL runtime reward signal is too coarse to guide autonomous improvements. A deterministic composite reward with per-dimension components is needed for better optimization signals while retaining safety hard-gate behavior.

## Scope
In scope:
- Add a deterministic reward breakdown function in `crates/tau-coding-agent/src/live_rl_runtime.rs`.
- Compute and persist composite + component reward attributes on `live.agent.decision` spans.
- Add unit + functional conformance tests for scoring and span persistence.

Out of scope:
- Policy-gradient algorithm changes.
- Training store schema changes.
- External API/wire-format changes.

## Acceptance Criteria
### AC-1 deterministic composite scoring
Given live run inputs (assistant reply presence, turn count, tool errors, safety blocked),
when reward is computed,
then output includes deterministic composite and component scores.

### AC-2 safety gate remains fail-closed
Given `safety_blocked = true`,
when reward is computed,
then composite reward remains `-1.0` and indicates safety penalty in the breakdown.

### AC-3 span attributes persist reward breakdown
Given a completed live run,
when `live.agent.decision` span is produced,
then span attributes include `reward` plus per-dimension breakdown keys.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Unit/Conformance | non-blocked run, no tool errors, short turns, assistant reply | reward breakdown function executes | composite + components are deterministic and bounded |
| C-02 | AC-2 | Unit/Regression | safety-blocked run | reward breakdown function executes | composite is `-1.0` and safety component indicates hard gate |
| C-03 | AC-3 | Functional/Conformance | completed live run emitted through runtime bridge | span is persisted | reward + component attributes exist and match deterministic values |

## Success Metrics / Observable Signals
- `cargo test -p tau-coding-agent spec_c05_unit_live_reward_breakdown_scores_deterministically`
- `cargo test -p tau-coding-agent spec_c06_functional_live_rollout_span_persists_reward_breakdown`
- `cargo fmt --check`
- `cargo clippy -p tau-coding-agent --no-deps -- -D warnings`
