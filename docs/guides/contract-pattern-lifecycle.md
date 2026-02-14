# Contract Pattern Lifecycle

<!-- architecture-doc:contract-pattern -->

This guide defines the shared contract-fixture lifecycle used across Tau runtime crates.

Source of truth:

- `crates/tau-contract/src/lib.rs`
- `crates/tau-custom-command/src/custom_command_contract.rs`
- `crates/tau-dashboard/src/dashboard_contract.rs`
- `crates/tau-gateway/src/gateway_contract.rs`
- `crates/tau-multi-channel/src/multi_channel_contract.rs`

## Purpose

Tau contracts provide deterministic, replayable fixtures for runtime behavior.

Core goals:

- Preserve stable input/output behavior across refactors
- Make malformed/retryable/success outcomes explicit
- Enforce schema + capability compatibility before runtime execution

## When to apply the pattern

Use a contract module when the runtime surface has all of the following:

- Structured external input payloads
- Policy/validation branches with strict failure semantics
- Expected output shape that should not drift silently

Do not use a contract module for internal helper logic that has no stable external behavior.

## Lifecycle flow

```mermaid
flowchart LR
    A[Fixture JSON] --> B[parse_*_contract_fixture]
    B --> C[validate_*_contract_fixture]
    C --> D[validate_*_contract_compatibility]
    D --> E[evaluate_*_case]
    E --> F[validate_*_case_result_against_contract]
    F --> G[Runtime summary and persistence]
```

Common shared helpers from `tau-contract`:

- `parse_fixture_with_validation`
- `load_fixture_from_path`
- `validate_fixture_header` / `validate_fixture_header_with_empty_message`
- `ensure_unique_case_ids`

## Extension process checklist

1. Add new enum/data shape to contract crate (`*_contract.rs`) with `Serialize`/`Deserialize`.
2. Extend `*_contract_capabilities` with explicit supported values.
3. Update `validate_*_contract_compatibility` for new values and fail-closed behavior.
4. Update `evaluate_*_case` and `validate_*_case_result_against_contract`.
5. Add fixture files in `testdata/*-contract`.
6. Add unit + functional + integration + regression tests for the new behavior.

## Anti-patterns

- Adding fixture fields without updating compatibility checks
- Treating unknown enum values as permissive defaults
- Skipping `validate_*_case_result_against_contract` in runtime loops
- Reusing one generic error code for multiple distinct policy failures

## Runnable validation snippets

```bash
# Shared fixture helper behavior
cargo test -p tau-contract

# Contract compatibility + replay checks
cargo test -p tau-gateway gateway_contract::tests::integration_gateway_contract_replay_is_deterministic_across_reloads
cargo test -p tau-custom-command custom_command_contract::tests::integration_custom_command_contract_replay_is_deterministic_across_reloads
cargo test -p tau-dashboard dashboard_contract::tests::integration_dashboard_contract_replay_is_deterministic_across_reloads
```

