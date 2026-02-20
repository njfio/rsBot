# Plan: Issue #2726 - G19 phase-2 API parity and force-layout rendering

## Approach
1. Add RED tests for `/api/memories/graph` authorized/unauthorized behavior and force-layout script markers.
2. Implement route compatibility path mapping to existing graph handler logic.
3. Replace static circular graph positioning in webchat with deterministic force-layout simulation.
4. Run scoped verification and update G19 checklist status entries.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/types.rs` (only if query/response fields require additive changes)
- `crates/tau-gateway/src/gateway_openresponses/webchat_page.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`
- `tasks/spacebot-comparison.md`

## Risks / Mitigations
- Risk: force-layout introduces non-deterministic rendering behavior.
  - Mitigation: deterministic initial seed/iteration count and bounded simulation.
- Risk: route alias could diverge from existing endpoint semantics.
  - Mitigation: share same handler and payload builder path.
- Risk: UI regressions in memory tab.
  - Mitigation: retain existing controls and status output, change only node-positioning algorithm.

## Interfaces / Contracts
- New route: `GET /api/memories/graph` with query parameters:
  - `session_key` (optional, defaults to `default`)
  - `max_nodes`
  - `min_edge_weight`
  - `relation_types`
- Existing route `GET /gateway/memory-graph/{session_key}` remains unchanged.

## ADR
- Not required: no new dependency family or protocol break.
