# Plan #2454

## Approach

1. Add lifecycle maintenance policy/result contracts to `tau-memory` runtime.
2. Implement deterministic maintenance pass over latest active records.
3. Reuse existing append-only + forgotten filtering semantics.
4. Add conformance tests in `runtime.rs` / `runtime/query.rs` and tool-facing regression checks.

## Affected Modules

- `crates/tau-memory/src/runtime.rs`
- `crates/tau-memory/src/runtime/query.rs`
- `crates/tau-memory/src/runtime/ranking.rs` (if needed for touched helpers)

## Risks / Mitigations

- Risk: aggressive pruning hides useful memories.
  Mitigation: identity exemption, configurable floor, soft-delete only.
- Risk: orphan detection misses inbound-only edges.
  Mitigation: build bidirectional edge-presence map per maintenance pass.
