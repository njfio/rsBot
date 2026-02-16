# Plan #2113

Status: Implemented
Spec: specs/2113/spec.md

## Approach

1. Add RED assertions for second-wave files to
   `scripts/dev/test-split-module-rustdoc.sh`.
2. Run guard script to capture expected failure before docs are added.
3. Add concise rustdoc comments to second-wave helper modules.
4. Run scoped guard + compile + targeted tests.

## Affected Modules

- `specs/milestones/m29/index.md`
- `specs/2113/spec.md`
- `specs/2113/plan.md`
- `specs/2113/tasks.md`
- `scripts/dev/test-split-module-rustdoc.sh`
- `crates/tau-github-issues/src/github_transport_helpers.rs`
- `crates/tau-github-issues/src/issue_filter.rs`
- `crates/tau-events/src/events_cli_commands.rs`
- `crates/tau-deployment/src/deployment_wasm_runtime.rs`

## Risks and Mitigations

- Risk: assertions become brittle on string-level doc changes.
  - Mitigation: use stable, API-specific marker phrases.
- Risk: broad file edits introduce accidental behavior drift.
  - Mitigation: docs-only changes + compile/targeted test matrix.

## Interfaces and Contracts

- Guard script:
  `bash scripts/dev/test-split-module-rustdoc.sh`
- Compile checks:
  `cargo check -p tau-github-issues --target-dir target-fast`
  `cargo check -p tau-events --target-dir target-fast`
  `cargo check -p tau-deployment --target-dir target-fast`
- Targeted tests:
  `cargo test -p tau-github-issues github_transport_helpers --target-dir target-fast`
  `cargo test -p tau-github-issues issue_filter --target-dir target-fast`
  `cargo test -p tau-events events_cli_commands --target-dir target-fast`
  `cargo test -p tau-deployment deployment_wasm_runtime --target-dir target-fast`

## ADR References

- Not required.
