# Issue Hierarchy Drift Rules

This guide defines the repository contract for detecting orphaned roadmap tasks and hierarchy drift across epics, stories, and tasks.

Machine-readable source of truth: `tasks/policies/issue-hierarchy-drift-rules.json`

## Required Metadata

Issues participating in roadmap hierarchy checks must satisfy:

- Required labels: `roadmap`, `testing-matrix`
- Hierarchy labels: one of `epic`, `story`, `task`
- Milestone required when hierarchy label is present
- Parent link field: `parent_issue_url`

Parent compatibility contract:

- `story` issues must link to an `epic` parent
- `task` issues must link to a `story` or `epic` parent

## Orphan Conditions

| Condition ID | Severity | Trigger |
| --- | --- | --- |
| `orphan.missing_parent_link` | error | Child issue has hierarchy label but no `parent_issue_url` |
| `orphan.parent_issue_not_found` | error | `parent_issue_url` cannot be resolved to an accessible issue |
| `orphan.parent_label_incompatible` | error | Child label and parent label violate allowed hierarchy mapping |

## Drift Conditions

| Condition ID | Severity | Trigger |
| --- | --- | --- |
| `drift.missing_required_labels` | warning | Required labels are missing |
| `drift.missing_milestone` | error | Hierarchy-labeled issue has no milestone |
| `drift.parent_milestone_mismatch` | warning | Parent/child milestones diverge without explicit waiver |
| `drift.parent_child_state_mismatch` | warning | Parent/child open/closed state indicates lifecycle drift |

## Remediation Playbook

`orphan.missing_parent_link`

- Add `parent_issue_url` in the tracked hierarchy source.
- Confirm the parent URL resolves with repository permissions.

`orphan.parent_issue_not_found`

- Replace stale parent link with canonical issue URL.
- If parent was intentionally removed, re-parent or close child with disposition notes.

`orphan.parent_label_incompatible`

- Correct child hierarchy label (`story`/`task`) to intended level.
- Re-link child to an allowed parent label type.

`drift.missing_required_labels`

- Reapply required labels (`roadmap`, `testing-matrix`).
- Re-run hierarchy drift checks after metadata correction.

`drift.missing_milestone`

- Assign current execution-wave milestone.
- Escalate to roadmap owner if no valid milestone exists.

`drift.parent_milestone_mismatch`

- Move child to parent milestone unless explicit cross-wave exception is approved.
- Document approved exception in tracker comments.

`drift.parent_child_state_mismatch`

- Align child state with lifecycle intent.
- If parent is closed and child remains active, re-parent child to active owner issue.
