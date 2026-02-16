# Plan #2139

Status: Implemented
Spec: specs/2139/spec.md

## Approach

1. Add RED wave-5 marker assertions to
   `scripts/dev/test-split-module-rustdoc.sh`.
2. Run guard script and capture expected failure.
3. Add concise rustdoc comments to wave-5 startup modules.
4. Run guard, compile checks, and targeted tests for touched crate.

## Affected Modules

- `specs/milestones/m32/index.md`
- `specs/2139/spec.md`
- `specs/2139/plan.md`
- `specs/2139/tasks.md`
- `scripts/dev/test-split-module-rustdoc.sh`
- `crates/tau-startup/src/startup_model_catalog.rs`
- `crates/tau-startup/src/startup_multi_channel_adapters.rs`
- `crates/tau-startup/src/startup_multi_channel_commands.rs`
- `crates/tau-startup/src/startup_rpc_capabilities_command.rs`

## Risks and Mitigations

- Risk: marker assertions become brittle.
  - Mitigation: assert stable phrases tied to API intent names.
- Risk: edits accidentally drift runtime behavior.
  - Mitigation: docs-focused changes plus compile/test matrix.

## Interfaces and Contracts

- Guard:
  `bash scripts/dev/test-split-module-rustdoc.sh`
- Compile:
  `cargo check -p tau-startup --target-dir target-fast`
- Targeted tests:
  `cargo test -p tau-startup startup_command_file_runtime --target-dir target-fast`
  `cargo test -p tau-startup startup_safety_policy --target-dir target-fast`

## ADR References

- Not required.
