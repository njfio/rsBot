# Milestone M40: Allow Pragmas Audit Wave 2

Status: Draft

## Objective

Audit active `allow(...)` pragmas under `crates/`, remove stale suppressions,
and publish retained rationale with reproducible verification evidence.

## Scope

In scope:

- wave-2 inventory of active `allow(...)` pragmas under `crates/`
- safe removal of stale suppressions in scoped modules
- documentation update capturing current inventory and retained rationale
- scoped compile/test/lint verification for touched crates

Out of scope:

- broad refactors to eliminate all remaining suppressions in one milestone
- behavior changes unrelated to lint/audit cleanup

## Success Signals

- M40 hierarchy exists and is active with epic/story/task/subtask linkage.
- stale suppressions targeted by wave-2 are removed safely.
- updated audit guide records current inventory and retained rationale.

## Issue Hierarchy

Milestone: GitHub milestone `M40 Allow Pragmas Audit Wave 2`

Epic:

- `#2200` Epic: M40 Allow Pragmas Audit Wave 2

Story:

- `#2201` Story: M40.1 Audit and reduce active allow pragmas

Task:

- `#2202` Task: M40.1.1 Remove stale allow pragmas and publish audit wave

Subtask:

- `#2203` Subtask: M40.1.1a Remove stale dead_code allow in PPO tests and update audit guide
