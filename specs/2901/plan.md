# Plan: Issue #2901 - ops chat assistant token-stream rendering contracts

## Approach
1. Add RED UI tests asserting assistant token-stream metadata and deterministic token row ordering contracts.
2. Add RED gateway integration tests seeding assistant content via persisted session entries and asserting `/ops/chat` token row contracts.
3. Implement minimal chat transcript rendering changes in `tau-dashboard-ui` to expose assistant token rows/metadata without altering existing role semantics.
4. Run required regression and verification gates (fmt/clippy/spec slices/mutation/live validation).

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations
- Risk: tokenization can produce brittle tests due to whitespace variance.
  - Mitigation: define normalized split behavior (`split_whitespace`) and assert deterministic ordered output.
- Risk: additive markup may regress existing markdown/tool-card assertions.
  - Mitigation: keep existing IDs/attributes unchanged and add separate token markers only for assistant rows.
- Risk: regression spread across sessions/detail routes.
  - Mitigation: rerun required regression suites before PR.

## Interface / Contract Notes
- No new routes.
- No API/wire-format changes.
- Additive SSR marker contracts only.
