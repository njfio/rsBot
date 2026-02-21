# Spec: Issue #3192 - correct inaccurate PPO unresolved-gap claim in whats-missing report

Status: Implemented

## Problem Statement
`tasks/whats-missing.md` currently claims PPO/GAE is not wired into runtime training. This is inaccurate and conflicts with implemented/runtime-tested PPO/GAE paths in `tau-coding-agent`.

## Scope
In scope:
- Remove/replace the inaccurate PPO unresolved-gap statement.
- Add evidence-backed wording aligned to current code.
- Update conformance markers in `scripts/dev/test-whats-missing.sh`.

Out of scope:
- Runtime algorithm changes.
- New training features.

## Acceptance Criteria
### AC-1 PPO statement reflects current implemented behavior
Given `tasks/whats-missing.md`,
when reviewing the PPO section,
then it must no longer claim PPO/GAE is unwired and must align with runtime evidence.

### AC-2 Conformance script enforces corrected marker language
Given `scripts/dev/test-whats-missing.sh`,
when it runs,
then it fails on stale PPO-unwired wording and passes on corrected marker wording.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional/Conformance | refreshed report | run conformance script | stale PPO-unwired marker is rejected |
| C-02 | AC-2 | Functional/Conformance | refreshed report + script | run conformance script | corrected PPO marker is required |

## Success Metrics / Observable Signals
- `scripts/dev/test-whats-missing.sh`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
