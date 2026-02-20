# Plan: Issue #2822 - Command-center connector health table SSR markers

## Approach
1. Add failing conformance tests proving connector row markers are absent.
2. Extend `tau-dashboard-ui` command-center snapshot model for connector health rows.
3. Render connector table section with deterministic row marker attributes and fallback row contract.
4. Extend gateway command-center snapshot mapping to include multi-channel connector rows from live connectors status.
5. Re-run phase-1A..1J regressions and full validation gates.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-gateway/src/gateway_openresponses/dashboard_status.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks & Mitigations
- Risk: collisions with existing command-center table markers.
  - Mitigation: use dedicated connector section ids/attributes without changing prior ids.
- Risk: inconsistent ordering for connector rows across map iteration.
  - Mitigation: rely on `BTreeMap` ordering and assert deterministic channel row ids.

## Interfaces / Contracts
- New UI snapshot row contract structure for connector health items:
  - channel
  - mode
  - liveness
  - events ingested
  - provider failures
- New SSR ids/markers:
  - `tau-ops-connector-health-table`
  - `tau-ops-connector-table-body`
  - `tau-ops-connector-row-<index>`
  - `data-channel`, `data-mode`, `data-liveness`, `data-events-ingested`, `data-provider-failures`

## ADR
No ADR required: no dependency, protocol, or architecture boundary change.
