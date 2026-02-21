# Spec: Issue #3220 - move gateway compat/telemetry runtime state into module

Status: Reviewed

## Problem Statement
`gateway_openresponses.rs` still owns compat/telemetry runtime-state structs and mutation/reporting methods inline. This keeps the root module larger than needed and mixes runtime-state concerns with routing composition.

## Scope
In scope:
- Extract OpenAI compat + UI telemetry runtime-state types/methods to a dedicated submodule.
- Rewire root module to use extracted logic.
- Tighten and enforce module-size guard.

Out of scope:
- Route/schema changes.
- New telemetry fields.
- Auth/rate-limit behavior changes.

## Acceptance Criteria
### AC-1 compat and telemetry status counters remain contract-stable
Given existing gateway integration scenarios for compat and UI telemetry,
when status is queried,
then runtime counters and reason-code maps match prior behavior.

### AC-2 size guard ratchet remains green after extraction
Given refactored module layout,
when running size guard script,
then root module line count is under tightened threshold and compat-state module wiring is enforced.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Conformance/Integration | openai compat status runtime fixture | `integration_gateway_status_endpoint_reports_openai_compat_runtime_counters` | compat counters/reason codes preserved |
| C-02 | AC-1 | Conformance/Integration | ui telemetry ingestion + status query | `integration_gateway_ui_telemetry_endpoint_persists_events_and_status_counters` | telemetry runtime counters/reason-code map preserved |
| C-03 | AC-2 | Functional/Regression | repo checkout | `scripts/dev/test-gateway-openresponses-size.sh` | tightened threshold + compat module wiring pass |

## Success Metrics / Observable Signals
- `scripts/dev/test-gateway-openresponses-size.sh`
- `cargo test -p tau-gateway integration_gateway_status_endpoint_reports_openai_compat_runtime_counters`
- `cargo test -p tau-gateway integration_gateway_ui_telemetry_endpoint_persists_events_and_status_counters`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
