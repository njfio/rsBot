# Spec #2050

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2050

## Problem Statement

The repository lacked concrete issue template files under
`.github/ISSUE_TEMPLATE`, preventing standardized intake execution aligned with
the AGENTS contract.

## Acceptance Criteria

- AC-1: Create `epic.md`, `story.md`, `task.md`, and `subtask.md`.
- AC-2: Non-epic templates include one-parent constraints.
- AC-3: Template metadata includes contract label guidance and type labels.
- AC-4: New template conformance tests pass.

## Scope

In:

- Add four issue templates.
- Validate via conformance tests.

Out:

- Changes to GitHub UI beyond markdown templates.

## Conformance Cases

- C-01 (AC-1, unit): 4/4 template files exist and are non-empty.
- C-02 (AC-2, functional): parent requirement guidance exists in
  `story.md`, `task.md`, `subtask.md`.
- C-03 (AC-3, integration): templates include type labels and namespace tokens.
- C-04 (AC-4, regression): template contract tests pass after implementation.

## Success Metrics

- Targeted template contract tests pass with zero failures.
- Full `.github/scripts` regression suite remains green.
