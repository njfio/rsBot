# Plan #2155

Status: Implemented
Spec: specs/2155/spec.md

## Approach

1. Add RED wave-7 marker assertions to
   `scripts/dev/test-split-module-rustdoc.sh`.
2. Run guard script and capture expected failure.
3. Add concise rustdoc comments to wave-7 helper modules.
4. Run guard, compile checks, and targeted tests for touched crates.

## Affected Modules

- `specs/milestones/m34/index.md`
- `specs/2155/spec.md`
- `specs/2155/plan.md`
- `specs/2155/tasks.md`
- `scripts/dev/test-split-module-rustdoc.sh`
- `crates/tau-onboarding/src/startup_daemon_preflight.rs`
- `crates/tau-onboarding/src/startup_resolution.rs`
- `crates/tau-tools/src/tool_policy_config.rs`
- `crates/tau-tools/src/tools/runtime_helpers.rs`

## Risks and Mitigations

- Risk: marker assertions become brittle.
  - Mitigation: assert stable phrases tied to API intent names.
- Risk: edits accidentally drift runtime behavior.
  - Mitigation: docs-focused changes plus compile/test matrix.

## Interfaces and Contracts

- Guard:
  `bash scripts/dev/test-split-module-rustdoc.sh`
- Compile:
  `cargo check -p tau-onboarding --target-dir target-fast`
  `cargo check -p tau-tools --target-dir target-fast`
- Targeted tests:
  `cargo test -p tau-onboarding startup_daemon_preflight --target-dir target-fast`
  `cargo test -p tau-onboarding startup_resolution --target-dir target-fast`
  `cargo test -p tau-tools tool_policy_config --target-dir target-fast`

## ADR References

- Not required.
