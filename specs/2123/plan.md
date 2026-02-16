# Plan #2123

Status: Implemented
Spec: specs/2123/spec.md

## Approach

1. Add RED wave-3 marker assertions to
   `scripts/dev/test-split-module-rustdoc.sh`.
2. Run guard script and capture expected failure.
3. Add concise rustdoc comments to wave-3 helper modules.
4. Run guard, compile checks, and targeted tests for touched crates.

## Affected Modules

- `specs/milestones/m30/index.md`
- `specs/2123/spec.md`
- `specs/2123/plan.md`
- `specs/2123/tasks.md`
- `scripts/dev/test-split-module-rustdoc.sh`
- `crates/tau-runtime/src/rpc_capabilities_runtime.rs`
- `crates/tau-runtime/src/rpc_protocol_runtime/transport.rs`
- `crates/tau-github-issues/src/issue_session_helpers.rs`
- `crates/tau-github-issues/src/issue_prompt_helpers.rs`

## Risks and Mitigations

- Risk: marker assertions become brittle.
  - Mitigation: assert stable phrases tied to API intent names.
- Risk: edits accidentally drift runtime behavior.
  - Mitigation: docs-focused changes plus compile/test matrix.

## Interfaces and Contracts

- Guard:
  `bash scripts/dev/test-split-module-rustdoc.sh`
- Compile:
  `cargo check -p tau-runtime --target-dir target-fast`
  `cargo check -p tau-github-issues --target-dir target-fast`
- Targeted tests:
  `cargo test -p tau-runtime rpc_capabilities_runtime --target-dir target-fast`
  `cargo test -p tau-runtime rpc_protocol_runtime::transport --target-dir target-fast`
  `cargo test -p tau-github-issues issue_session_helpers --target-dir target-fast`
  `cargo test -p tau-github-issues issue_prompt_helpers --target-dir target-fast`

## ADR References

- Not required.
