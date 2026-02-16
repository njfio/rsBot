# Plan #2097

Status: Reviewed
Spec: specs/2097/spec.md

## Approach

1. Identify a contiguous execution-domain flag slice suitable for first
   migration.
2. Add/normalize `crates/tau-cli/src/cli_args/execution_domain_flags.rs` and
   wire it into `cli_args.rs`.
3. Migrate the selected slice with minimal behavior drift and compile impact.
4. Run targeted regression checks for parsing/help compatibility and task-level
   verification.

## Affected Modules

- `crates/tau-cli/src/cli_args.rs`
- `crates/tau-cli/src/cli_args/execution_domain_flags.rs`
- `specs/2097/spec.md`
- `specs/2097/plan.md`
- `specs/2097/tasks.md`
- `specs/milestones/m27/index.md`

## Risks and Mitigations

- Risk: clap flatten/field migration breaks parsing behavior.
  - Mitigation: test-first regression checks and bounded first slice.
- Risk: downstream call-sites assume flat field ownership in `Cli`.
  - Mitigation: choose migration strategy that preserves field-level API or
    updates call-sites in same PR with compile checks.

## Interfaces and Contracts

- `cargo test -p tau-cli`
- targeted cli parsing/help regression tests under tau-cli/tau-coding-agent
- task-scoped `cargo check` for affected crates

## ADR References

- Not required.
