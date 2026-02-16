# Plan #2147

Status: Implemented
Spec: specs/2147/spec.md

## Approach

1. Add RED wave-6 marker assertions to
   `scripts/dev/test-split-module-rustdoc.sh`.
2. Run guard script and capture expected failure.
3. Add concise rustdoc comments to wave-6 onboarding modules.
4. Run guard, compile checks, and targeted tests for touched crate.

## Affected Modules

- `specs/milestones/m33/index.md`
- `specs/2147/spec.md`
- `specs/2147/plan.md`
- `specs/2147/tasks.md`
- `scripts/dev/test-split-module-rustdoc.sh`
- `crates/tau-onboarding/src/onboarding_command.rs`
- `crates/tau-onboarding/src/onboarding_daemon.rs`
- `crates/tau-onboarding/src/onboarding_paths.rs`
- `crates/tau-onboarding/src/onboarding_profile_bootstrap.rs`

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
- Targeted tests:
  `cargo test -p tau-onboarding onboarding_command --target-dir target-fast`
  `cargo test -p tau-onboarding onboarding_daemon --target-dir target-fast`
  `cargo test -p tau-onboarding onboarding_paths --target-dir target-fast`
  `cargo test -p tau-onboarding onboarding_profile_bootstrap --target-dir target-fast`

## ADR References

- Not required.
