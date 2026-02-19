# Spec: Issue #2617 - Memory graph visualization API and dashboard view

Status: Implemented

## Problem Statement
Gateway webchat currently supports raw memory read/write only. Operators lack a graph export and visual relation view for memory content, making it hard to inspect structure, overlap, and density signals during live operations.

## Acceptance Criteria

### AC-1 Gateway exposes memory graph export endpoint with filtering controls
Given a gateway memory session,
When operators call `GET /gateway/memory-graph/{session_key}` with optional filters,
Then the API returns a deterministic graph payload with nodes/edges, relation types, sizing weights, and applied filter metadata.

### AC-2 Webchat memory view renders memory graph with relation-type and sizing cues
Given the webchat memory tab,
When operators load a graph,
Then the UI renders a graph visualization with node size cues and edge relation-type cues and shows export metadata/status.

### AC-3 Default behavior remains stable and fail-closed auth/policy boundaries are preserved
Given existing gateway auth flows and memory endpoints,
When memory graph features are not used,
Then existing behavior is unchanged and graph endpoint enforces same auth/rate-limit boundaries.

### AC-4 Scoped verification gates are green
Given the memory graph changes,
When formatting, linting, and targeted gateway tests run,
Then all checks pass.

## Scope

### In Scope
- Add gateway memory graph GET endpoint with query-based filtering.
- Derive deterministic graph structure from persisted memory text.
- Add webchat memory-tab controls for filter inputs and graph loading.
- Render an operator-facing in-browser graph with relation colors and node-size scaling.
- Add/extend tests for endpoint and webchat shell integration.

### Out of Scope
- New external visualization dependencies.
- Global memory ontology redesign.
- Cross-session memory federation graph.

## Conformance Cases
- C-01 (unit): webchat HTML includes memory graph endpoint + rendering controls.
- C-02 (functional): memory graph endpoint returns graph payload for persisted memory content.
- C-03 (regression): memory graph endpoint rejects unauthorized requests and preserves existing memory read/write behavior.
- C-04 (integration): query filters (`max_nodes`, `min_edge_weight`, `relation_types`) deterministically shape returned edge/node sets.
- C-05 (verify): `cargo fmt --check`, `cargo clippy -p tau-gateway -- -D warnings`, `cargo test -p tau-gateway gateway_openresponses` pass.

## Success Metrics / Observable Signals
- Operators can inspect memory relation structure from webchat without external tooling.
- Graph payloads are deterministic and filterable for repeatable operational diagnostics.
- Existing session/memory API paths remain backward compatible.
