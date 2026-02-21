# Spec: Issue #2992 - Split cli_args runtime/deployment flag block into include artifacts

Status: Implemented

## Problem Statement
`crates/tau-cli/src/cli_args.rs` is a large hotspot that slows review and increases regression risk. The post-`execution_domain` runtime/deployment flag declarations are a cohesive slice that can be extracted without changing external CLI contracts.

## Acceptance Criteria

### AC-1 Runtime feature flag declarations are extracted from root file
Given the `Cli` parser struct,
When implementation is complete,
Then the post-`execution_domain` runtime/deployment field declarations live in dedicated `crates/tau-cli/src/cli_args/*` artifacts and are included from `cli_args.rs`.

### AC-2 CLI parse contract is preserved
Given existing CLI flags,
When parsing arguments,
Then long names, env vars, defaults, and value parsing behavior for extracted fields remain unchanged.

### AC-3 Focused regressions remain green
Given tau CLI/runtime consumers,
When scoped regression suites run,
Then `cargo test -p tau-cli` and `cargo test -p tau-coding-agent cli_validation -- --test-threads=1` pass.

### AC-4 Root hotspot size decreases materially
Given baseline root line count,
When extraction completes,
Then `crates/tau-cli/src/cli_args.rs` line count is reduced by at least 400 lines.

## Scope

### In Scope
- Extract the tail runtime/deployment/events/rpc flag declarations out of `cli_args.rs`.
- Keep field names and clap attributes unchanged.
- Keep downstream consumers unchanged unless required by compile safety.
- Add/adjust split guardrail tests if needed for this phase.

### Out of Scope
- Renaming flags or changing CLI semantics.
- Provider/model catalog behavior changes.
- Additional feature work outside structural decomposition.

## Conformance Cases
- C-01: Extracted field declaration artifact(s) exist and are wired into `Cli`.
- C-02: `cargo test -p tau-cli` passes.
- C-03: `cargo test -p tau-coding-agent cli_validation -- --test-threads=1` passes.
- C-04: Root `cli_args.rs` line count reduction >= 400 lines from baseline.

## Success Metrics / Observable Signals
- `cargo fmt --check` passes.
- `cargo clippy -p tau-cli -- -D warnings` passes.
- Conformance cases C-01..C-04 pass.

## Approval Gate
P1 scope: spec authored/reviewed by agent; implementation proceeds and is flagged for human review in PR.
