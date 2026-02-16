# Spec #2035

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2035

## Problem Statement

The existing dependency drift checker validated parent linkage and a minimal
label set, but did not enforce AGENTS-required namespace labels
(`type:`, `area:`, `process:`, `priority:`, `status:`). This allowed metadata
drift in newly created roadmap issues.

## Acceptance Criteria

- AC-1: Validation tooling checks required namespace label prefixes.
- AC-2: Validation tooling continues to enforce hierarchy parent-link rules.
- AC-3: Policy contract includes namespace requirements and remediation.
- AC-4: CI/local validation tests fail when namespace/hierarchy drift is
  introduced.

## Scope

In:

- Extend `tasks/policies/issue-hierarchy-drift-rules.json`.
- Extend `scripts/dev/dependency-drift-check.sh`.
- Extend drift-check test fixtures and policy contract tests.
- Update operator docs describing drift rules.

Out:

- Backfilling legacy historical issues.

## Conformance Cases

- C-01 (AC-1, functional): Missing namespace prefixes trigger
  `drift.missing_required_label_prefixes`.
- C-02 (AC-2, functional): Parent-link and compatibility rules still detect
  orphan and incompatible parent states.
- C-03 (AC-3, unit): Policy file includes namespace prefixes and subtask
  hierarchy rule entries.
- C-04 (AC-4, regression): Drift-check fixture tests pass and fail with expected
  condition markers.

## Success Metrics

- `scripts/dev/test-dependency-drift-check.sh` passes.
- `.github/scripts/test_issue_hierarchy_drift_rules.py` passes.
- Full `.github/scripts` unittest suite remains green.
