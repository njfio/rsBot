# Plan: Issue #2682 - PRD gateway audit summary and audit log endpoints

## Approach
1. Add gateway constants/routes for `/gateway/audit/summary` and `/gateway/audit/log`.
2. Implement dedicated audit runtime handlers in `tau-gateway` that:
   - enforce auth/rate limits,
   - load dashboard action and UI telemetry JSONL records,
   - normalize both sources into one audit record model,
   - apply deterministic filtering and pagination,
   - return summary aggregates and invalid-record counters.
3. Extend `/gateway/status` `gateway.web_ui` discovery payload with audit endpoint metadata.
4. Add RED-first integration/regression tests for C-01..C-08 and make them pass.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/audit_runtime.rs` (new)
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`
- `specs/milestones/m110/index.md`

## Risks / Mitigations
- Risk: malformed JSONL lines break audit reads.
  - Mitigation: line-by-line parse with invalid-line counters and fail-open per-line behavior.
- Risk: unbounded record loading impacts responsiveness.
  - Mitigation: bounded pagination defaults and capped `page_size`.
- Risk: filter semantics drift.
  - Mitigation: deterministic normalization and explicit conformance tests for filter combinations.

## Interfaces / Contracts
- `GET /gateway/audit/summary`
  - Query: `since_unix_ms`, `until_unix_ms`
  - Response: merged totals + per-source/action/view/reason counters + invalid-record counts.
- `GET /gateway/audit/log`
  - Query: `page`, `page_size`, `source`, `action`, `view`, `reason_code`, `since_unix_ms`, `until_unix_ms`
  - Response: merged newest-first records + pagination metadata.
- `/gateway/status` additions under `gateway.web_ui`:
  - `audit_summary_endpoint`
  - `audit_log_endpoint`

## ADR
- Not required (bounded additive API slice, no new dependency/protocol).
- Human review requested in PR because this is a P1 scope.
