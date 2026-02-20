# Plan: Issue #2818 - Command-center alert feed list SSR markers

## Approach
1. Add conformance tests that fail against current alert feed summary-only markup.
2. Extend `tau-dashboard-ui` command-center snapshot model to carry deterministic alert feed row list context.
3. Render alert feed row list markers (`id`, `data-alert-code`, `data-alert-severity`, message text) and fallback row when alerts are empty.
4. Extend gateway command-center snapshot mapping to provide alert row list context from `dashboard_status` alerts.
5. Re-run phase-1A..1I command-center suites and full validation gates.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-gateway/src/gateway_openresponses/dashboard_status.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks & Mitigations
- Risk: marker regressions in existing alert feed contracts.
  - Mitigation: keep existing `data-primary-alert-*` markers and add row list markers in addition.
- Risk: brittle test assertions due attribute ordering.
  - Mitigation: assert explicit deterministic contract substrings matching rendered attribute order.

## Interfaces / Contracts
- New UI snapshot row contract structure for alert feed items:
  - alert code
  - alert severity
  - alert message
- New SSR ids/markers:
  - `tau-ops-alert-feed-list`
  - `tau-ops-alert-row-<index>`
  - `data-alert-code`
  - `data-alert-severity`

## ADR
No ADR required: no dependency, protocol, or architecture boundary change.
