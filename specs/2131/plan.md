# Plan #2131

Status: Implemented
Spec: specs/2131/spec.md

## Approach

1. Add RED wave-4 marker assertions to
   `scripts/dev/test-split-module-rustdoc.sh`.
2. Run guard script and capture expected failure.
3. Add concise rustdoc comments to wave-4 helper modules.
4. Run guard, compile checks, and targeted tests for touched crates.

## Affected Modules

- `specs/milestones/m31/index.md`
- `specs/2131/spec.md`
- `specs/2131/plan.md`
- `specs/2131/tasks.md`
- `scripts/dev/test-split-module-rustdoc.sh`
- `crates/tau-runtime/src/rpc_protocol_runtime/dispatch.rs`
- `crates/tau-runtime/src/rpc_protocol_runtime/parsing.rs`
- `crates/tau-runtime/src/runtime_output_runtime.rs`
- `crates/tau-github-issues/src/issue_run_error_comment.rs`

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
  `cargo test -p tau-runtime parse_rpc_frame --target-dir target-fast`
  `cargo test -p tau-runtime dispatch_rpc_frame_maps_supported_kinds_to_response_envelopes --target-dir target-fast`
  `cargo test -p tau-runtime summarize_message --target-dir target-fast`
  `cargo test -p tau-runtime event_to_json --target-dir target-fast`
  `cargo test -p tau-github-issues issue_run_error_comment --target-dir target-fast`

## ADR References

- Not required.
