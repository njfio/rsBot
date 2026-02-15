# Issue 1769 Tasks

Status: Implementing

## Ordered Tasks

T1 (tests-first): add contract tests for template fields, rubric schema, and
docs discoverability.

T2: add critical-path update markdown template with required status/blocker/risk
sections.

T3: add risk rubric policy JSON with low/med/high definitions and rationale
requirements.

T4: update roadmap docs to reference template/rubric usage for tracker comments.

T5: run targeted and regression test matrix; capture evidence in issue/PR.

## Tier Mapping

- Unit: template/rubric existence and schema assertions
- Functional: required field and level checks
- Integration: docs references across roadmap guides/index
- Regression: missing required snippets fail deterministic contract tests
- Conformance: C-01..C-04 mapping validated in test file
