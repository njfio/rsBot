# Plan #2050

Status: Implemented
Spec: specs/2050/spec.md

## Approach

Create the template directory and four markdown templates with standardized
front matter and mandatory intake sections. Validate with red/green test loop.

## Affected Modules

- `.github/ISSUE_TEMPLATE/epic.md`
- `.github/ISSUE_TEMPLATE/story.md`
- `.github/ISSUE_TEMPLATE/task.md`
- `.github/ISSUE_TEMPLATE/subtask.md`
- `.github/scripts/test_issue_template_contract.py`

## Risks and Mitigations

- Risk: Missing token/section in one template causes contract drift.
  - Mitigation: Enforce all templates through shared contract test assertions.

## Interfaces and Contracts

- GitHub markdown issue template files are the interface surface.
- Python unittest assertions are the conformance contract.

## ADR References

- Not required.
