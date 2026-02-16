# Plan #2034

Status: Reviewed
Spec: specs/2034/spec.md

## Approach

Implement a template-first intake contract:

1. Create `.github/ISSUE_TEMPLATE/` and add `epic.md`, `story.md`, `task.md`,
   and `subtask.md`.
2. Encode required field blocks in each template:
   parent, milestone, dependencies, risk, labels, DoR.
3. Add/extend validation tests that assert required fields and label namespaces
   are present in each template.
4. Update related docs/policy references if template paths are currently absent.

## Affected Modules

- `.github/ISSUE_TEMPLATE/epic.md`
- `.github/ISSUE_TEMPLATE/story.md`
- `.github/ISSUE_TEMPLATE/task.md`
- `.github/ISSUE_TEMPLATE/subtask.md`
- Template validation tests (new or existing under `scripts/dev` or `.github/scripts`)
- Optional docs references in `docs/guides/issue-hierarchy-drift-rules.md`

## Risks and Mitigations

- Risk: Template text drifts from AGENTS contract over time.
  - Mitigation: Add deterministic lint/tests that gate required fields/tokens.
- Risk: Overly rigid templates reduce issue-author usability.
  - Mitigation: Keep concise required fields and include one-line examples.
- Risk: Legacy labels confuse intake authors.
  - Mitigation: Explicitly list required namespaced labels in each template.

## Interfaces and Contracts

- Intake contract surface: GitHub issue markdown templates.
- Validation contract: presence of required sections + labels + hierarchy notes.
- Failure mode: lint/test command exits non-zero with actionable diagnostics.

## ADR References

- Not required. No architectural or dependency change beyond issue intake docs.
