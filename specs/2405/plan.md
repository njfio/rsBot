# Plan: Issue #2405 - Restore fail-closed OpenResponses preflight budget gate

## Approach
1. Reproduce RED failures for C-01/C-02 on `tau-gateway`.
2. Update gateway OpenResponses agent preflight configuration to preserve fail-closed enforcement
   while preventing context-compaction from masking budget breaches.
3. Re-run targeted tests C-01/C-02/C-03 and critical-gap script segment.
4. Finalize lifecycle updates and PR evidence.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs` (verification only if needed)

## Risks / Mitigations
- Risk: changing budget knobs alters normal request behavior.
  - Mitigation: keep change scoped to preflight config wiring and validate success schema regression
    test C-03.
- Risk: provider dispatch might still occur on rejected path.
  - Mitigation: enforce C-02 panic-guard test.

## Interfaces / Contracts
- No API shape change.
- Preserves existing error code/message contract for gateway runtime preflight failure.

## ADR
- Not required; no architectural or dependency change.
