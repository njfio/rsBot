# Plan #2310

Status: Reviewed
Spec: specs/2310/spec.md

## Approach

1. Add RED integration tests in gateway OpenResponses test suite for token-preflight failure and fail-fast semantics.
2. Introduce a small helper deriving token ceilings from configured `max_input_chars` and set `max_estimated_input_tokens` plus `max_estimated_total_tokens` in gateway `AgentConfig`.
3. Keep char-limit validation and response shape unchanged; only add local preflight budget configuration.
4. Run scoped verification gates (`fmt`, `clippy`, gateway test suites), then map ACs to tests.

## Affected Modules

- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations

- Risk: token limit derivation too strict could block valid requests.
  - Mitigation: derive from existing char-limit budget and validate a within-budget success case.
- Risk: error mapping drift could break compatibility assumptions.
  - Mitigation: assert existing response envelope still returned for success paths.

## Interfaces / Contracts

- Gateway OpenResponses request execution keeps existing wire schema.
- `AgentConfig` preflight fields are now populated in gateway request path.
