# Plan: Issue #2984 - gateway config runtime extraction

## Approach
1. Record RED baseline hotspot size and run scoped config test baseline.
2. Create `config_runtime.rs` and move config handlers + tightly-coupled helper plumbing.
3. Wire handler imports in `gateway_openresponses.rs` with no route changes.
4. Run targeted config regressions + quality gates + sanitized live validation.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/config_runtime.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs` (if visibility/import wiring requires adjustment)

## Risks and Mitigations
- Risk: subtle change in override file / policy file handling behavior.
  - Mitigation: pure move + scoped config tests.
- Risk: route payload contract drift.
  - Mitigation: targeted endpoint tests and no schema edits.

## Interfaces / Contracts
- No endpoint path or schema changes.
- No auth policy changes.
