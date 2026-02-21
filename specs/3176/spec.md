# Spec: Issue #3176 - tau-gaps report stale-claim resynchronization

Status: Accepted

## Problem Statement
`tasks/tau-gaps-issues-improvements.md` reports stale status in multiple sections, including repo hygiene and testing depth, and `scripts/dev/test-tau-gaps-issues-improvements.sh` enforces older marker expectations.

## Scope
In scope:
- Refresh stale report entries using current repository evidence.
- Update conformance script assertions to match refreshed report markers.
- Preserve report structure while correcting stale status claims.

Out of scope:
- Runtime behavior changes.
- New roadmap decisions outside evidence synchronization.

## Acceptance Criteria
### AC-1 Repository hygiene claims reflect current tracked docs
Given `tasks/tau-gaps-issues-improvements.md`,
when repository hygiene rows are reviewed,
then `CONTRIBUTING.md` and `SECURITY.md` are not marked missing and are evidenced as done.

### AC-2 Under-tested snapshot indicators reflect current test depth improvements
Given `tasks/tau-gaps-issues-improvements.md`,
when testing gap table rows are reviewed,
then `tau-training-proxy` and `kamn-core` counts reflect current improved signals and recommendations focus remaining weak areas.

### AC-3 Conformance script enforces refreshed markers and rejects stale wording
Given `scripts/dev/test-tau-gaps-issues-improvements.sh`,
when executed,
then it passes for refreshed markers and fails on stale missing-claim markers.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional/Conformance | refreshed repo hygiene section | inspect rows | CONTRIBUTING/SECURITY marked done with path evidence |
| C-02 | AC-2 | Functional/Conformance | refreshed testing-gap table | inspect rows | training-proxy and kamn-core counts updated to current signals |
| C-03 | AC-3 | Conformance | updated report + script | run script | script passes and stale-claim asserts remain blocked |

## Success Metrics / Observable Signals
- `scripts/dev/test-tau-gaps-issues-improvements.sh`
