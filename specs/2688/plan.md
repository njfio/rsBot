# Plan: Issue #2688 - PRD gateway training rollouts and config endpoints

## Approach
1. Extend `gateway_openresponses` route constants/router wiring with training rollouts/config endpoints.
2. Add small training runtime helpers in a dedicated module for:
   - Rollout JSONL loading with malformed-line accounting and pagination.
   - Training config override read/write persistence under `.tau/training/config-overrides.json`.
3. Add handler implementations with existing auth/error patterns (`authorize_dashboard_request` + deterministic `OpenResponsesApiError`).
4. Extend `/gateway/status` `gateway.web_ui` discovery metadata.
5. Add conformance/regression tests first (RED), then implement (GREEN), then run scoped validation gates.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`
- `crates/tau-gateway/src/gateway_openresponses/training_runtime.rs` (new)

## Risks / Mitigations
- Risk: rollout artifact schema drift.
  - Mitigation: tolerant parser with field defaults + malformed-line counters.
- Risk: config patch mutation of unsupported fields.
  - Mitigation: strict whitelist + deterministic `400` when no supported fields accepted.
- Risk: pagination abuse.
  - Mitigation: bounded `page`/`per_page` validation and hard caps.

## Interfaces / Contracts
- `GET /gateway/training/rollouts?page=<u64>&per_page=<u64>`
  - `200`: `{ schema_version, generated_unix_ms, page, per_page, total_records, total_pages, invalid_records, records, diagnostics }`
- `PATCH /gateway/training/config`
  - request: optional subset of `{ enabled, update_interval_rollouts, max_rollouts_per_update, max_failure_streak, store_path }`
  - `200`: `{ accepted, applied, pending_overrides, overrides_path, updated_unix_ms }`
  - `400`: deterministic validation error on invalid/unsupported payload.

## ADR
- Not required. No dependency, protocol, or architecture decision change.
