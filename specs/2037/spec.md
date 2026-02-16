# Spec #2037

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2037

## Problem Statement

Roadmap status blocks in tracked planning docs were stale relative to live
GitHub issue/milestone state, causing operator confusion and false progress
signals.

## Acceptance Criteria

- AC-1: `tasks/todo.md` status block reflects current GitHub closure state.
- AC-2: `tasks/tau-vs-ironclaw-gap-list.md` status block reflects current
  GitHub closure state.
- AC-3: `scripts/dev/roadmap-status-sync.sh --check` exits cleanly after sync.

## Scope

In:

- Run roadmap status sync generator.
- Verify updated status blocks.
- Verify check mode.

Out:

- Rewriting non-generated planning narrative sections.

## Conformance Cases

- C-01 (AC-1, functional): `tasks/todo.md` generated block updates to all-closed
  roadmap status snapshot.
- C-02 (AC-2, functional): `tasks/tau-vs-ironclaw-gap-list.md` generated block
  updates child-task and epic closure markers.
- C-03 (AC-3, regression): `scripts/dev/roadmap-status-sync.sh --check --quiet`
  passes immediately after sync.

## Success Metrics

- Sync command reports both docs updated.
- Check mode passes with zero diff output.
