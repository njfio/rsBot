# Issue 1644 Plan

Status: Reviewed

## Approach

1. Tests-first:
   - add `scripts/dev/test-safety-diagnostics-telemetry.sh` to validate
     required source/tests/docs markers for safety diagnostics payload and schema
     contracts
   - run harness for RED against missing quickstart diagnostics section
2. Add quickstart section with operator-facing safety telemetry inspection
   commands and sample output fields.
3. Run harness for GREEN.
4. Run targeted tests:
   - `cargo test -p tau-runtime unit_event_to_json_maps_safety_policy_applied_shape`
   - `cargo test -p tau-diagnostics unit_summarize_audit_file_accepts_prompt_telemetry_v1_schema`
   - `cargo test -p tau-diagnostics functional_summarize_audit_file_accepts_legacy_prompt_telemetry_fixture`
   - `cargo test -p tau-diagnostics regression_summarize_audit_file_ignores_future_prompt_telemetry_schema_versions`
   - `cargo test -p tau-agent-core integration_prompt_safety_blocks_tool_output_before_reinjection`
5. Run scoped checks:
   - `scripts/dev/roadmap-status-sync.sh --check --quiet`
   - `cargo fmt --check`
   - `cargo clippy -p tau-runtime -- -D warnings`
   - `cargo clippy -p tau-diagnostics -- -D warnings`

## Affected Areas

- `scripts/dev/test-safety-diagnostics-telemetry.sh`
- `docs/guides/quickstart.md`
- `specs/1644/spec.md`
- `specs/1644/plan.md`
- `specs/1644/tasks.md`

## Risks And Mitigations

- Risk: docs drift from runtime payload shape.
  - Mitigation: harness checks docs tokens and runtime JSON mapping test names.
- Risk: schema/version compatibility assumptions drift.
  - Mitigation: targeted tau-diagnostics schema tests are part of issue gate.

## Interfaces / Contracts

- Runtime event contract: `event_to_json` must preserve
  `safety_policy_applied` payload shape fields.
- Diagnostics schema contract: telemetry v1 + legacy compatibility behavior
  remains explicit via tests/docs.

## ADR

No dependency/protocol/architecture decision changes; ADR not required.
