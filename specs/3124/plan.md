# Plan: Issue #3124 - ops job cancel action contracts

## Approach
1. Add RED conformance tests for cancel action markers in dashboard UI and gateway route output.
2. Extend ops shell controls query parsing with `cancel_job` request contract.
3. Resolve deterministic cancel outcomes in gateway jobs fixture rows.
4. Extend dashboard chat snapshot + UI rendering to expose cancel action and cancel panel contracts.
5. Run scoped regressions for existing tools/jobs contracts.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-dashboard-ui/src/tests.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_shell_controls.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_dashboard_shell.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks & Mitigations
- Risk: cancel contract mutates deterministic jobs fixtures in ways that regress `2100`/`2101`.
  - Mitigation: run `spec_3116` and `spec_3120` regressions after implementation.
- Risk: ambiguous outcomes for non-running jobs.
  - Mitigation: encode explicit deterministic statuses (`cancelled`, `not-cancellable`, `not-found`) in panel contracts.

## Interfaces / Contracts
- Added query control: `cancel_job`
- Added snapshot fields:
  - `job_cancel_requested_job_id`
  - `job_cancel_status`
- Added shell markers:
  - `#tau-ops-jobs-cancel-<index>`
  - `#tau-ops-job-cancel-panel`
  - `#tau-ops-job-cancel-submit`

## ADR
No ADR required.
