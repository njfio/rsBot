# Plan #2032

Status: Reviewed
Spec: specs/2032/spec.md

## Approach

Execute decomposition in repeatable pairs for each oversized file:

1. Planning subtask (`*a`) publishes split map and ownership boundaries.
2. Execution subtask (`*b`) extracts domains and updates guardrails with parity
   evidence.

Current wave status:

- `cli_args.rs`: planning complete (`#2058`), execution in progress (`#2059`).
- Remaining files (`benchmark_artifact.rs`, `tools.rs`,
  `github_issues_runtime.rs`, `channel_store_admin.rs`) pending.

## Affected Modules

- `crates/tau-cli/src/cli_args.rs` and `crates/tau-cli/src/cli_args/*`
- `crates/tau-coding-agent/src/benchmark_artifact.rs`
- `crates/tau-tools/src/tools.rs`
- `crates/tau-github-issues-runtime/src/github_issues_runtime.rs`
- `crates/tau-channel-store/src/channel_store_admin.rs`
- decomposition guardrail scripts under `scripts/dev/`

## Risks and Mitigations

- Risk: large file moves create hidden behavior regressions.
  - Mitigation: run split guardrail tests + scoped crate checks after each move.
- Risk: decomposition stalls due long compile/test feedback loops.
  - Mitigation: prioritize fast guardrail loops first, then capture heavier
    integration evidence in dedicated runs.

## Interfaces and Contracts

- Per-task split-map artifact contract (JSON + Markdown + schema/test).
- Per-task guardrail contract scripts enforcing line thresholds and marker
  invariants.

## ADR References

- Not required.
