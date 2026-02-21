# Plan: Issue #3120 - ops job detail output contracts

## Approach
1. Extend dashboard chat snapshot with selected job detail output fields.
2. Add job selection query parsing (`job`) in ops shell controls.
3. Resolve selected job detail from jobs rows and render deterministic detail markers.
4. Add RED-first conformance tests for UI and gateway route contracts.
5. Run scoped regressions for jobs list + tools detail contracts.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-dashboard-ui/src/tests.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_shell_controls.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_dashboard_shell.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks & Mitigations
- Risk: Selection fallback mismatch when requested job id is absent/invalid.
  - Mitigation: deterministic fallback to first jobs row.
- Risk: Regressing existing jobs list/tool detail markers.
  - Mitigation: rerun `spec_3116` and `spec_3112` regressions.

## Interfaces / Contracts
- Added query control: `job`
- Added snapshot fields:
  - `job_detail_selected_job_id`
  - `job_detail_status`
  - `job_detail_duration_ms`
  - `job_detail_stdout`
  - `job_detail_stderr`

## ADR
No ADR required.
