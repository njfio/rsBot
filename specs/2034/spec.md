# Spec #2034

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2034

## Problem Statement

The repository contract in `AGENTS.md` requires standardized intake templates for
epic/story/task/subtask issues under `.github/ISSUE_TEMPLATE`. The repository
currently lacks these templates, creating drift in hierarchy metadata, required
labels, DoR fields, and parent linkage quality.

## Acceptance Criteria

- AC-1: Epic, story, task, and subtask issue templates exist at
  `.github/ISSUE_TEMPLATE/{epic,story,task,subtask}.md`.
- AC-2: Each template includes required intake fields:
  parent link (except epic), milestone, dependencies, risk level, required
  labels, and definition-of-ready checklist items.
- AC-3: Template guidance enforces hierarchy:
  milestone -> epic -> story -> task -> subtask and exactly one parent for
  task/subtask.
- AC-4: Template examples use the required label namespaces:
  type, area, process, priority, status.
- AC-5: Template content is deterministic and validated by tests/fixtures.

## Scope

In:

- Add four markdown issue templates.
- Add template-focused tests/validation fixtures.
- Add docs references to the new intake flow where needed.

Out:

- GitHub UI form YAML migration.
- Retrofitting historical closed issues.

## Conformance Cases

- C-01 (AC-1, unit): All four template files are present and non-empty.
- C-02 (AC-2, functional): Template sections contain required field headers and
  prompts for parent/milestone/dependencies/risk/labels/DoR.
- C-03 (AC-3, functional): story/task/subtask templates include parent-link
  instructions and hierarchy rules.
- C-04 (AC-4, regression): Required namespaced label tokens appear in template
  examples and are not replaced by legacy unlabeled forms.
- C-05 (AC-5, integration): Template lint/test command passes and fails on
  deliberately malformed fixture input.

## Success Metrics

- 4/4 required template files present.
- Conformance tests for C-01 through C-05 pass.
- New issues created after rollout consistently include required metadata
  without manual correction.
