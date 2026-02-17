# Spec: Issue #2405 - Restore fail-closed OpenResponses preflight budget gate

Status: Accepted

## Problem Statement
Critical-gap validation currently fails because OpenResponses preflight does not reject an
over-budget request. `integration_spec_c01_openresponses_preflight_blocks_over_budget_request`
expects gateway failure semantics but receives a successful 200 response, and the provider-dispatch
guard in C-02 is bypassed.

## Acceptance Criteria

### AC-1 Over-budget preflight requests fail closed
Given an OpenResponses request that exceeds configured preflight token budget,
When `/v1/responses` is called,
Then the gateway returns contract-consistent failure status and payload, without generating a
successful response object.

### AC-2 Preflight rejection path skips provider dispatch
Given an over-budget request and a panic-on-dispatch mock provider,
When preflight runs,
Then the request is rejected before provider invocation.

### AC-3 Existing successful schema behavior remains intact
Given an in-budget request,
When `/v1/responses` is called,
Then success response schema assertions continue to pass unchanged.

## Scope

### In Scope
- Gateway OpenResponses preflight budget enforcement configuration.
- Targeted gateway conformance/regression tests for preflight rejection and skip-dispatch behavior.

### Out of Scope
- Changes to tokenizer heuristics in `tau-agent-core`.
- Provider protocol/transport behavior.
- Global budget policy defaults outside OpenResponses preflight.

## Conformance Cases

| Case | AC | Tier | Input | Expected |
|---|---|---|---|---|
| C-01 | AC-1 | Integration | `integration_spec_c01_openresponses_preflight_blocks_over_budget_request` | HTTP failure (gateway runtime error) with token-budget message |
| C-02 | AC-2 | Integration | `integration_spec_c02_openresponses_preflight_skips_provider_dispatch` with panic client | Request fails preflight; panic provider is not invoked |
| C-03 | AC-3 | Regression | `regression_spec_c03_openresponses_preflight_preserves_success_schema` | Success schema remains unchanged |

## Success Metrics / Observable Signals
- C-01/C-02/C-03 pass on `tau-gateway`.
- Gateway segment in `scripts/dev/verify-critical-gaps.sh` passes.
- No regressions in scoped `fmt` / `clippy` for touched crates.
