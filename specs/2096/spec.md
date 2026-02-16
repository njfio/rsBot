# Spec #2096

Status: Implemented
Milestone: specs/milestones/m27/index.md
Issue: https://github.com/njfio/Tau/issues/2096

## Problem Statement

Task M27.1.1 tracks extraction of CLI execution-domain flag groups from
`crates/tau-cli/src/cli_args.rs` into a dedicated module while preserving
runtime/startup behavior. Subtask `#2097` delivered the migration in PR `#2099`.
This task closes by validating that implementation against task-level
acceptance criteria.

## Acceptance Criteria

- AC-1: Execution-domain flags are extracted into a dedicated module under
  `crates/tau-cli/src/cli_args/`.
- AC-2: `Cli` wiring preserves clap parsing/compatibility for migrated flags.
- AC-3: Startup and events command paths remain behaviorally compatible for the
  migrated flag slice.

## Scope

In:

- consume merged implementation from `#2097` / PR `#2099`
- map task ACs to concrete conformance cases/tests
- publish task-level closure artifacts and evidence

Out:

- full CLI domain decomposition in one task
- semantic changes to event command behavior

## Conformance Cases

- C-01 (AC-1, integration):
  `crates/tau-cli/src/cli_args/execution_domain_flags.rs` exists and
  `cli_args.rs` wires it via `#[command(flatten)]`.
- C-02 (AC-2, functional):
  `cargo check -p tau-cli --lib --target-dir target-fast` passes.
- C-03 (AC-2/AC-3, regression):
  `bash scripts/dev/test-cli-args-domain-split.sh` passes.
- C-04 (AC-3, integration): startup/event regression suite
  `cargo test -p tau-coding-agent startup_preflight_and_policy --target-dir target-fast`
  passes.

## Success Metrics

- Task issue `#2096` closes with conformance evidence linked to PR `#2099`.
- `specs/2096/{spec,plan,tasks}.md` lifecycle is completed.
- M27 parent story handoff is unblocked.
