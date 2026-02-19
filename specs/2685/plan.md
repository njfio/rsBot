# Plan: Issue #2685 - PRD gateway training status endpoint

## Approach
1. Add gateway constant/route for `/gateway/training/status`.
2. Implement handler that enforces auth/rate limits and returns training status from the existing dashboard snapshot loader.
3. Add `/gateway/status` web UI discovery metadata for the new endpoint.
4. Add RED-first integration/regression tests for success path, unavailable fallback, unauthorized behavior, and status discovery.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`
- `specs/milestones/m110/index.md`

## Risks / Mitigations
- Risk: duplicate training-report logic diverges across endpoints.
  - Mitigation: reuse existing dashboard snapshot training report in the new handler.
- Risk: endpoint ambiguity when training status is missing.
  - Mitigation: preserve deterministic `status_present=false` payload and diagnostics.

## Interfaces / Contracts
- `GET /gateway/training/status`
  - Response: deterministic training report payload (`status_present`, `run_state`, counts, diagnostics, source path metadata).
- `/gateway/status` additions under `gateway.web_ui`:
  - `training_status_endpoint`

## ADR
- Not required (bounded additive API slice, no dependency/protocol changes).
- Human review requested in PR because this is a P1 scope.
