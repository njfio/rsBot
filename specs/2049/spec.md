# Spec #2049

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2049

## Problem Statement

Before template implementation, no concrete intake-field contract existed in the
repository for issue templates. This caused repeated drift in milestone/parent/
label metadata quality for newly created issues.

## Acceptance Criteria

- AC-1: The required field contract is explicitly represented in issue template
  structure for epic/story/task/subtask.
- AC-2: Required fields include milestone, dependencies, risk, labels, and DoR
  checklists across all templates.
- AC-3: Parent constraints are explicitly documented for non-epic templates.
- AC-4: Namespace label tokens are documented in each template.

## Scope

In:

- Define and encode the intake field contract inside template files.

Out:

- Historical issue backfill.

## Conformance Cases

- C-01 (AC-1, functional): Each template contains explicit required sections.
- C-02 (AC-2, functional): `Milestone`, `Dependencies`, `Risk`,
  `Required Labels`, and `Definition of Ready` appear in all templates.
- C-03 (AC-3, regression): `story.md`, `task.md`, and `subtask.md` include
  `Parent:` guidance plus "exactly one parent" language.
- C-04 (AC-4, integration): Namespace tokens (`type:`, `area:`, `process:`,
  `priority:`, `status:`) appear in all templates and are validated by tests.

## Success Metrics

- Template contract test suite passes for all template files and field checks.
