# Plan: Issue #2917 - ops memory create-entry contracts

## Approach
1. Add RED UI tests for deterministic create-form control markers.
2. Add RED gateway integration tests submitting create-form payloads and asserting persisted/discoverable entries.
3. Implement minimal create form snapshot/render contracts and gateway form-handling route plumbing.
4. Run regression + verify gates (fmt/clippy/spec slices/mutation/live validation).

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_dashboard_shell.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_shell_controls.rs` (if needed for post-submit state)
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations
- Risk: introducing new POST route contract can regress existing ops route behavior.
  - Mitigation: additive endpoint and stable defaults; preserve current route markers.
- Risk: full-field mapping can be incomplete or silently malformed.
  - Mitigation: integration tests assert persisted values via route-level contracts.
- Risk: regression in memory search/scope/type contracts.
  - Mitigation: rerun memory regression slice including `spec_2905`, `spec_2909`, `spec_2913`.

## Interface / Contract Notes
- Adds ops-shell form submission handling for memory create contracts.
- No new external API dependency.
