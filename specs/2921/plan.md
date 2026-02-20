# Plan: Issue #2921 - ops memory edit-entry contracts

## Approach
1. Add RED UI tests for deterministic edit-form control markers.
2. Add RED gateway integration tests submitting edit-form payloads and asserting existing entry update behavior.
3. Implement minimal edit form rendering + gateway form-handling route plumbing.
4. Run regression + verify gates (fmt/clippy/spec slices/mutation/live validation).

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_dashboard_shell.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_shell_controls.rs` (if edit-status controls expand)
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations
- Risk: edit handler can accidentally create new entries or mutate wrong IDs.
  - Mitigation: integration tests must pre-create fixture and assert targeted ID update semantics.
- Risk: full-field update mapping can silently drop fields.
  - Mitigation: read-back assertions for summary/tags/facts/source/scope/type/importance/relations.
- Risk: regression in existing memory contracts.
  - Mitigation: rerun regression slice including `spec_2905`, `spec_2909`, `spec_2913`, `spec_2917`.

## Interface / Contract Notes
- Adds ops-shell form submission handling for memory edit contracts.
- No new external API dependency.
