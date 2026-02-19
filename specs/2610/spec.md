# Spec: Issue #2610 - Branch hygiene wave for stale merged remote branches

Status: Reviewed

## Problem Statement
Merged remote branches accumulate over time and increase operational noise for active lane triage. The repository needs a deterministic, auditable procedure that identifies stale merged branches and supports safe pruning with explicit destructive confirmation plus rollback guidance.

## Acceptance Criteria

### AC-1 Deterministic stale merged branch inventory is generated
Given a repository with remote branches merged into the base branch,
When the branch hygiene script runs in dry-run mode,
Then it emits machine-readable and Markdown inventory artifacts listing candidate branches and skip reasons.

### AC-2 Remote branch deletion is fail-closed and explicit
Given stale merged branch candidates are present,
When the script is run without explicit destructive confirmation,
Then no remote deletions occur.
When explicit deletion mode and confirmation are provided,
Then only eligible non-protected stale merged branches are deleted.

### AC-3 Audit + rollback controls are documented and recorded
Given pruning execution is requested,
When deletion is performed,
Then audit output includes branch name and tip commit SHA for recovery, and docs include rollback steps to restore deleted branches.

### AC-4 Scoped verification gates are green
Given the new script/tests/docs,
When scoped checks run,
Then script contract tests and formatting/lint checks pass.

## Scope

### In Scope
- New `scripts/dev` stale merged branch prune automation with dry-run default.
- Deterministic report outputs under `tasks/reports/`.
- Script contract test coverage for dry-run, explicit-delete guard, and protected-branch handling.
- Documentation updates for rollback and audit workflow.

### Out of Scope
- Automatic scheduled deletion in CI.
- Non-merged stale branch cleanup policy changes.
- GitHub API-based branch operations (local git remote refs only for this slice).

## Conformance Cases
- C-01 (functional): script dry-run emits deterministic inventory artifacts with candidates/skips.
- C-02 (regression): script refuses deletions without explicit confirmation flags.
- C-03 (integration): script deletes only eligible branches when explicit execution flags are provided and records tip SHAs for rollback.
- C-04 (verify): script test + lint/style checks pass.

## Success Metrics / Observable Signals
- `scripts/dev/test-stale-merged-branch-prune.sh` passes.
- Inventory artifacts are emitted in both JSON and Markdown with non-empty schema fields.
- Protected branches are never deleted in integration test fixture.
