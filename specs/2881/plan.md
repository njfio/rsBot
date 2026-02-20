# Plan: Issue #2881 - chat multi-line input contracts

## Approach
1. Add additive multiline compose contract markers (Shift+Enter hint/attributes) to Tau Ops chat textarea in `tau-dashboard-ui`.
2. Ensure gateway chat send path preserves embedded newline payloads (while still failing closed for blank/whitespace-only sends).
3. Add UI and gateway conformance tests for multiline markers, newline preservation, and hidden-panel route behavior.
4. Re-run required prior chat regression suites and verification gates.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_dashboard_shell.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations
- Risk: chat send behavior regression for whitespace-only messages.
  - Mitigation: retain trim-based emptiness guard while preserving full payload for non-empty messages.
- Risk: fragile HTML assertions.
  - Mitigation: assert deterministic markers/IDs and focused payload markers only.

## Interface / Contract Notes
- No new routes.
- Additive UI marker contracts only.
- Existing `/ops/chat/send` endpoint semantics maintained, with explicit multiline payload preservation validation.
