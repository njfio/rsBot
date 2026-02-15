# Critical-Path Update Template

Use this template for recurring critical-path updates in `#1678` and milestone
execution threads.

## Update Metadata

- Update Date (UTC): `<YYYY-MM-DDTHH:MM:SSZ>`
- Wave/Milestone: `<e.g. #21 Structural Runtime Hardening>`
- Update Owner: `<@handle>`
- Reporting Window: `<start -> end>`

## Allowed Status Values

`blocked|at-risk|on-track|done`

## Allowed Risk Values

`low|med|high`

## Critical Path Items

| Critical Path Item | Status | Blockers | Owner | Risk Score | Risk Rationale | Next Action | Target Date |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `<issue-id and title>` | `<blocked|at-risk|on-track|done>` | `<none or blocker links>` | `<owner>` | `<low|med|high>` | `<one-sentence rationale>` | `<next execution step>` | `<YYYY-MM-DD>` |

## Blockers And Dependency Notes

- Blockers:
  - `<issue link>` -> `<impact>` -> `<required unblock action>`
- Dependency drift/orphan checks:
  - `scripts/dev/dependency-drift-check.sh --mode dry-run`

## Risks And Mitigations

- Risk Score Summary:
  - `<low|med|high>`
- Rationale:
  - `<why this score applies>`
- Mitigation/rollback trigger:
  - `<what action changes score downward>`

## Commitments Before Next Update

- Next Action: `<single highest-priority deliverable>`
- Target Date: `<YYYY-MM-DD>`
- Merge/validation evidence expected: `<PR/issue links>`
