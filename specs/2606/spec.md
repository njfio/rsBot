# Spec: Issue #2606 - Validate tau-gaps roadmap items and execute open P0/P1 remediations

Status: Implemented

## Problem Statement
Story #2606 tracks the M104 revalidation/remediation wave. With all child task issues now merged, the roadmap and milestone artifacts must be reconciled to avoid stale "open/partial" status for already delivered work.

## Acceptance Criteria

### AC-1 Tau-gaps roadmap document reflects delivered M104 outcomes
Given `tasks/tau-gaps-issues-improvements.md`,
When M104 closures are reconciled,
Then previously open/partial items now completed in this wave are marked done with concrete evidence references.

### AC-2 Milestone index reflects actual closure status
Given `specs/milestones/m104/index.md`,
When all story task children are complete,
Then milestone status/issue map/exit criteria are updated to match current repository and issue state.

### AC-3 Story closeout evidence is recorded
Given Story #2606,
When closure is prepared,
Then issue comments capture outcome/PR/spec/test/conformance summary and status label transitions to done.

### AC-4 Scoped validation gates are green
Given this docs/governance closeout slice,
When docs quality and formatting checks run,
Then checks pass.

## Scope

### In Scope
- Update `tasks/tau-gaps-issues-improvements.md` completion statuses for newly closed tasks.
- Update `specs/milestones/m104/index.md` status and exit criteria to align with merged work.
- Record closeout evidence on #2606 and prepare #2605 closure.

### Out of Scope
- New runtime feature implementation.
- Reopening or redesigning M104 task scope.

## Conformance Cases
- C-01 (functional): roadmap status table marks items #16 and #17 as done with evidence links.
- C-02 (functional): milestone index status and exit criteria align with closed M104 tasks.
- C-03 (functional): issue #2606 has closure summary comment and `status:done` label.
- C-04 (verify): docs quality workflow/check commands pass for changed artifacts.

## Success Metrics / Observable Signals
- No stale "open/partial" status remains for #2614/#2615 in roadmap artifacts.
- M104 issue map and exit criteria are internally consistent with GitHub issue state.
