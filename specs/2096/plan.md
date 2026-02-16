# Plan #2096

Status: Implemented
Spec: specs/2096/spec.md

## Approach

1. Use merged subtask implementation from `#2097` as source of truth.
2. Produce task-level lifecycle artifacts (`spec/plan/tasks`) with AC mapping.
3. Re-run scoped CLI decomposition and startup/event regression checks.
4. Close task with status updates and parent-story handoff.

## Affected Modules

- `specs/2096/spec.md`
- `specs/2096/plan.md`
- `specs/2096/tasks.md`
- `crates/tau-cli/src/cli_args.rs`
- `crates/tau-cli/src/cli_args/execution_domain_flags.rs`
- `scripts/dev/test-cli-args-domain-split.sh`
- `crates/tau-startup/src/lib.rs`
- `crates/tau-events/src/events_cli_commands.rs`

## Risks and Mitigations

- Risk: task closure drifts from merged subtask behavior.
  - Mitigation: rerun task-scoped verification commands on latest `master`.
- Risk: compatibility regressions hide in startup preflight paths.
  - Mitigation: execute `startup_preflight_and_policy` target suite.

## Interfaces and Contracts

- Split guard:
  `bash scripts/dev/test-cli-args-domain-split.sh`
- Compile contract:
  `cargo check -p tau-cli --lib --target-dir target-fast`
- Startup/event regression:
  `cargo test -p tau-coding-agent startup_preflight_and_policy --target-dir target-fast`

## ADR References

- Not required.
