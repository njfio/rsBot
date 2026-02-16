# Plan #2163

Status: Implemented
Spec: specs/2163/spec.md

## Approach

1. Add RED wave-8 marker assertions to
   `scripts/dev/test-split-module-rustdoc.sh`.
2. Run guard script and capture expected failure.
3. Add concise rustdoc comments to wave-8 gateway split helper modules.
4. Run guard, scoped checks, and targeted gateway tests.

## Affected Modules

- `specs/milestones/m35/index.md`
- `specs/2163/spec.md`
- `specs/2163/plan.md`
- `specs/2163/tasks.md`
- `scripts/dev/test-split-module-rustdoc.sh`
- `crates/tau-gateway/src/gateway_openresponses/openai_compat.rs`
- `crates/tau-gateway/src/gateway_openresponses/request_translation.rs`
- `crates/tau-gateway/src/gateway_openresponses/types.rs`
- `crates/tau-gateway/src/gateway_openresponses/dashboard_status.rs`

## Risks and Mitigations

- Risk: marker assertions become brittle.
  - Mitigation: assert stable phrases tied to API intent names.
- Risk: docs edit accidentally changes behavior.
  - Mitigation: docs-only line additions plus scoped compile/test matrix.

## Interfaces and Contracts

- Guard:
  `bash scripts/dev/test-split-module-rustdoc.sh`
- Compile:
  `cargo check -p tau-gateway --target-dir target-fast`
- Targeted tests:
  `cargo test -p tau-gateway unit_translate_openresponses_request_supports_item_input_and_function_call_output --target-dir target-fast`
  `cargo test -p tau-gateway unit_translate_chat_completions_request_maps_messages_and_session_seed --target-dir target-fast`
  `cargo test -p tau-gateway functional_apply_gateway_dashboard_action_writes_control_and_audit_records --target-dir target-fast`

## ADR References

- Not required.
