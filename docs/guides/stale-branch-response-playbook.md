# Stale Branch Response Playbook

This guide defines stale-branch thresholds, automated alert conditions, and the acknowledge/resolve workflow.

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

## Active PR Usage

Active PRs must include stale-branch status fields in `.github/pull_request_template.md`:

- `Branch Freshness`
- `Stale Alert Acknowledgement`
