# Plan #2049

Status: Implemented
Spec: specs/2049/spec.md

## Approach

Represent the field contract as deterministic markdown sections inside each
issue template. Validate via Python contract tests to avoid future drift.

## Affected Modules

- `.github/ISSUE_TEMPLATE/epic.md`
- `.github/ISSUE_TEMPLATE/story.md`
- `.github/ISSUE_TEMPLATE/task.md`
- `.github/ISSUE_TEMPLATE/subtask.md`
- `.github/scripts/test_issue_template_contract.py`

## Risks and Mitigations

- Risk: Field headings diverge from expected contract.
  - Mitigation: Add explicit heading assertions in contract tests.

## Interfaces and Contracts

- Contract interface: issue-template markdown content and headings.
- Validation interface: unittest checks for required sections and tokens.

## ADR References

- Not required.
