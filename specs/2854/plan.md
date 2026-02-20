# Plan: Issue #2854 - command-center route visibility contracts

## Approach
1. Add route-aware visibility computation for command-center panel in `tau-dashboard-ui`.
2. Extend UI and gateway tests with spec-derived route visibility assertions.
3. Revalidate prior command-center/chat/sessions contracts with targeted regressions.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations
- Risk: route visibility changes may unintentionally affect existing panel contracts.
  - Mitigation: rerun command-center/chat/sessions regression suites.
- Risk: brittle marker ordering in tests.
  - Mitigation: use deterministic attribute ordering and stable marker IDs.

## Interface / Contract Notes
- Additive visibility marker contract only (`aria-hidden` + route metadata).
- No endpoint or schema changes.
