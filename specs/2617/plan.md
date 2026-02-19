# Plan: Issue #2617 - Memory graph visualization API and dashboard view

## Approach
1. Add RED tests for endpoint availability/auth behavior and webchat graph controls.
2. Implement backend graph export endpoint and deterministic graph builder from memory text.
3. Implement memory-tab graph controls and SVG rendering with relation and size cues.
4. Run scoped verify gates and map AC/C evidence in PR.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/types.rs`
- `crates/tau-gateway/src/gateway_openresponses/webchat_page.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`
- `specs/2617/spec.md`
- `specs/2617/plan.md`
- `specs/2617/tasks.md`

## Risks / Mitigations
- Risk: noisy/non-deterministic graph layout or relation extraction.
  - Mitigation: deterministic extraction and stable sorting; simple SVG placement strategy.
- Risk: UI regression in memory tab.
  - Mitigation: additive controls only; existing memory load/save flow untouched.
- Risk: auth bypass on new endpoint.
  - Mitigation: reuse existing `authorize_and_enforce_gateway_limits` flow and add regression tests.

## Interfaces / Contracts
- New endpoint: `GET /gateway/memory-graph/{session_key}`
- Query filters:
  - `max_nodes` (usize)
  - `min_edge_weight` (f64)
  - `relation_types` (comma-separated relation names)
- Response fields: `session_key`, `exists`, `node_count`, `edge_count`, `nodes[]`, `edges[]`, `filters{...}`.

## ADR
- Not required: no dependency or architecture boundary change.
