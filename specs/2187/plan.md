# Plan #2187

Status: Implemented
Spec: specs/2187/spec.md

## Approach

1. Add RED wave-11 marker assertions to
   `scripts/dev/test-split-module-rustdoc.sh`.
2. Run guard script and capture expected failure.
3. Add concise rustdoc comments to wave-11 provider auth runtime modules.
4. Run guard, scoped check, and targeted provider tests.

## Affected Modules

- `specs/milestones/m38/index.md`
- `specs/2187/spec.md`
- `specs/2187/plan.md`
- `specs/2187/tasks.md`
- `scripts/dev/test-split-module-rustdoc.sh`
- `crates/tau-provider/src/auth_commands_runtime/anthropic_backend.rs`
- `crates/tau-provider/src/auth_commands_runtime/openai_backend.rs`
- `crates/tau-provider/src/auth_commands_runtime/google_backend.rs`
- `crates/tau-provider/src/auth_commands_runtime/shared_runtime_core.rs`

## Risks and Mitigations

- Risk: marker assertions become brittle.
  - Mitigation: assert stable phrases tied to API intent names.
- Risk: docs edit accidentally changes behavior.
  - Mitigation: docs-only line additions plus scoped compile/test matrix.

## Interfaces and Contracts

- Guard:
  `bash scripts/dev/test-split-module-rustdoc.sh`
- Compile:
  `cargo check -p tau-provider --target-dir target-fast`
- Targeted tests:
  `cargo test -p tau-provider regression_execute_auth_login_rejects_launch_for_api_key_mode --target-dir target-fast`
  `cargo test -p tau-provider functional_execute_auth_login_launches_openai_cli_with_json_output --target-dir target-fast`
  `cargo test -p tau-provider integration_execute_auth_login_google_adc_launch_uses_gcloud_cli --target-dir target-fast`

## ADR References

- Not required.
