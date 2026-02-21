# Plan: Issue #3116 - ops jobs list contracts

## Approach
1. Extend `tau-dashboard-ui` chat snapshot model with deterministic jobs summary + jobs row fields.
2. Render new `/ops/tools-jobs` jobs panel/summary/table markers in UI shell output.
3. Populate jobs snapshot rows in gateway ops shell collector with deterministic fixture-compatible values.
4. Add RED-first conformance tests for UI and gateway route contracts.
5. Run scoped regression suites and quality gates.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-dashboard-ui/src/tests.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_dashboard_shell.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks & Mitigations
- Risk: Marker collisions with existing tools detail panel IDs.
  - Mitigation: Use dedicated `tau-ops-jobs-*` marker namespace.
- Risk: Route-level regressions for existing tools detail contracts.
  - Mitigation: Rerun `spec_3112` in both UI and gateway suites.

## Interfaces / Contracts
- UI snapshot additions:
  - `jobs_summary_running_count`
  - `jobs_summary_completed_count`
  - `jobs_summary_failed_count`
  - `jobs_rows`
- Jobs row contract fields:
  - `job_id`, `job_name`, `job_status`, `started_unix_ms`, `finished_unix_ms`

## ADR
No ADR required (no dependency, architecture, or protocol changes).
