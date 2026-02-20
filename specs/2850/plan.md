# Plan: Issue #2850 - command-center recent-cycles table contracts

## Approach
1. Extend `tau-dashboard-ui` recent-cycles table markup with deterministic panel and summary-row marker attributes and explicit empty-state row behavior.
2. Add UI functional tests and gateway functional/integration tests to cover AC/C cases.
3. Preserve existing command-center behavior and validate with targeted regressions.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations
- Risk: marker string-order instability may break deterministic assertions.
  - Mitigation: assert stable, explicit attribute ordering in render path and test against exact marker substrings.
- Risk: regressions in existing command-center route contracts.
  - Mitigation: rerun prior command-center spec suites as regression gate.

## Interface / Contract Notes
- Additive SSR marker attributes only; no endpoint/transport/schema changes.
