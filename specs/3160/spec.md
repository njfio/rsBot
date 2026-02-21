# Spec: Issue #3160 - Sync Review #35 property-gap closure wording and conformance guard

Status: Reviewed

## Problem Statement
`tasks/review-35.md` still marks property-based testing as only "Improved" with a remaining-gap summary, despite merged wave-2 property invariants in PR `#3157`. `scripts/dev/test-review-35.sh` encodes the stale wording.

## Scope
In scope:
- Update Review #35 closure table row for property-based testing to done.
- Update Review #35 remaining summary to indicate no remaining items in the tracker.
- Update `scripts/dev/test-review-35.sh` expected markers to enforce the new state.

Out of scope:
- Additional runtime/test behavior changes.
- New property-test implementations.

## Acceptance Criteria
### AC-1 Review #35 property row reflects merged closure state
Given `tasks/review-35.md`,
when closure table rows are reviewed,
then `Property-based testing` is marked `**Done**`.

### AC-2 Review #35 remaining summary no longer claims open property gap
Given `tasks/review-35.md`,
when the closure summary line is reviewed,
then it reports no remaining items in this tracker.

### AC-3 Conformance script enforces updated Review #35 closure markers
Given updated Review #35 wording,
when `scripts/dev/test-review-35.sh` executes,
then it passes for updated markers and rejects stale wording.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | review-35 closure table | inspect property row | row value is `**Done**` |
| C-02 | AC-2 | Functional | review-35 summary line | inspect remaining summary | summary states no remaining items |
| C-03 | AC-3 | Conformance | updated review-35 + script | run `scripts/dev/test-review-35.sh` | script passes and stale marker checks remain enforced |

## Success Metrics / Observable Signals
- `scripts/dev/test-review-35.sh`
- `cargo fmt --check`
