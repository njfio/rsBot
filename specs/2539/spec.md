# Spec #2539 - Epic: G16 phase-3 profile policy hot-reload bridge

Status: Implemented

## Problem Statement
`tasks/spacebot-comparison.md` keeps `G16` open because Tau lacks a bounded closure slice that maps live profile-policy changes into active runtime heartbeat behavior without restart.

## Acceptance Criteria
### AC-1 Bounded scope is explicit
Given the phase-3 G16 objective, when work is executed, then scope is limited to active profile policy change detection + runtime heartbeat reload bridging.

### AC-2 Child chain is complete and traceable
Given this epic, when delivery lands, then story/task/subtask artifacts and AC-mapped evidence exist and close in milestone M93.

## Scope
In scope:
- M93 issue hierarchy and binding specs.
- Runtime heartbeat policy bridge from profile store changes.

Out of scope:
- Full profile-wide hot reload across all modules.
- G17 template watcher behavior.

## Conformance Cases
- C-01 (AC-1): `specs/milestones/m93/index.md` defines bounded in/out scope.
- C-02 (AC-2): `specs/2541/spec.md` AC map and `specs/2542/*` evidence are complete.

## Success Metrics
- #2539/#2540/#2541/#2542 close with `status:done`.
- M93 checklist/doc updates completed.
