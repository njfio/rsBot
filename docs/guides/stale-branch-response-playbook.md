# Stale Branch Response Playbook

This guide defines stale-branch thresholds, automated alert conditions, conflict triage flow, and rollback criteria.

Machine-readable source of truth: `tasks/policies/stale-branch-alert-policy.json`

## Threshold Matrix

| Severity | Branch age threshold | Behind-commit threshold |
| --- | --- | --- |
| `warning` | `>= 2` days | `>= 25` commits |
| `critical` | `>= 5` days | `>= 75` commits |

Persistent merge conflict warning threshold: `12` hours unresolved.

## Automated Alert Conditions

| Condition ID | Severity | Trigger | Alert channels |
| --- | --- | --- | --- |
| `stale.warning.age_or_behind` | warning | Branch age or behind count reaches warning threshold | `github.pr_comment`, `github.issue_comment:#1678` |
| `stale.critical.age_or_behind` | error | Branch age or behind count reaches critical threshold | `github.pr_comment`, `github.issue_comment:#1678`, `runtime.maintainer_page` |
| `stale.warning.unresolved_conflict` | warning | Merge conflict remains unresolved for >= 12 hours | `github.pr_comment`, `github.issue_comment:#1678` |

## Acknowledge/Resolve Workflow

A stale alert must be acknowledged within `4` hours.

Required acknowledgement fields:

- `owner`
- `acknowledged_at`
- `next_update_at`
- `mitigation_plan`

Allowed resolve states:

- `rebased_and_green`
- `merged`
- `closed_not_planned`

Suggested response steps:

1. Add/update the acknowledgement comment on the PR with required fields.
2. Update status tags (`stale-warning` or `stale-critical`) on the tracker issue comment.
3. Post next-update timestamp and execute mitigation plan.
4. Clear stale status only after one resolve state is reached.

## Conflict Triage Flow

Use this triage flow for high-conflict branches:

1. Classify conflict scope (`manifest`, `workflow`, `runtime module`, `docs-only`).
2. Estimate resolution effort and blast radius within 30 minutes.
3. Select one decision path and record rationale in PR comments:
   - `merge`
   - `rebase`
   - `abandon`

## Merge/Rebase/Abandon Criteria

`merge` when:

- Branch is <= 10 commits behind `master`.
- Conflict scope is docs-only or low-risk non-hotspot changes.
- No rollback trigger is active.

`rebase` when:

- Branch is > 10 commits behind `master`.
- Conflict scope includes hotspot paths or generated artifacts.
- Rebase can complete in one review cycle.

`abandon` when:

- Branch is > 21 days old with unresolved conflicts.
- PR is superseded or roadmap scope is obsolete.
- Two failed rebases occurred within 48 hours.

## Rollback Trigger Conditions

| Trigger ID | Trigger | Required actions |
| --- | --- | --- |
| `rollback.conflict_churn` | Same conflict appears in two consecutive rebases | Pause impacted lane; escalate; open rollback/disposition issue |
| `rollback.red_ci_after_resolution` | Quality checks fail after conflict resolution without source changes | Revert resolution commit; restore last green; rerun validation matrix |
| `rollback.hotspot_collision` | Hotspot files conflict across more than one active lane | Freeze non-owner merges; serialize hotspot owner PRs; rebaseline stale branches |

## Active PR Usage

Active PRs must include stale-branch status fields in `.github/pull_request_template.md`:

- `Branch Freshness`
- `Stale Alert Acknowledgement`
- `Conflict Response Decision`
- `Rollback Trigger Check`
