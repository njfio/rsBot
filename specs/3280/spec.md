# Spec: Issue #3280 - move gateway root utility helpers to dedicated module

Status: Reviewed

## Problem Statement
`gateway_openresponses.rs` still defines two utility helpers. These can be extracted into a dedicated module so root remains focused on module composition and wiring.

## Scope
In scope:
- Move `derive_gateway_preflight_token_limit` and `validate_gateway_openresponses_bind` to `gateway_openresponses/root_utilities.rs`.
- Preserve helper behavior and execution-path call sites.
- Ratchet and enforce root-module size/ownership guard.

Out of scope:
- Algorithm/validation behavior changes.
- Endpoint contract changes.
- Additional runtime behavior modifications.

## Acceptance Criteria
### AC-1 helper behavior remains stable
Given existing regression/integration tests,
when tests run,
then bind validation behavior and execution-path usage behavior remain unchanged.

### AC-2 root-module ownership boundaries improve
Given refactored module layout,
when root guard runs,
then root line count is under tightened threshold and both helper function definitions are no longer declared in root.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Regression/Conformance | invalid bind string | `regression_validate_gateway_openresponses_bind_rejects_invalid_socket_address` | bind parse helper continues failing closed |
| C-02 | AC-1 | Integration/Conformance | openresponses execution path requiring preflight token limit | `integration_spec_c01_openresponses_request_persists_session_usage_summary` | execution path behavior remains stable |
| C-03 | AC-2 | Functional/Regression | repo checkout | `scripts/dev/test-gateway-openresponses-size.sh` | tightened threshold + ownership checks pass |

## Success Metrics / Observable Signals
- `scripts/dev/test-gateway-openresponses-size.sh`
- `cargo test -p tau-gateway regression_validate_gateway_openresponses_bind_rejects_invalid_socket_address`
- `cargo test -p tau-gateway integration_spec_c01_openresponses_request_persists_session_usage_summary`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
