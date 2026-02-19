# Plan #2592

## Approach
1. Introduce a canonical relation enum in `tau-memory` and migrate runtime relation payloads to typed relation values.
2. Update relation normalization and backend hydration to parse/serialize canonical relation strings with explicit validation errors.
3. Add bounded BFS graph traversal scoring seeded by initial ranked candidates.
4. Extend RRF fusion path to include graph-ranked candidates and remove additive graph-after-fusion scoring.
5. Add conformance/regression tests across `tau-memory` and `tau-tools`, then update G6 checklist lines.

## Affected Modules
- `crates/tau-memory/src/runtime.rs`
- `crates/tau-memory/src/runtime/query.rs`
- `crates/tau-memory/src/runtime/ranking.rs`
- `crates/tau-memory/src/runtime/backend.rs`
- `crates/tau-tools/src/tools/memory_tools.rs`
- `crates/tau-tools/src/tools/tests.rs`
- `tasks/spacebot-comparison.md`

## Risks & Mitigations
- Risk: relation schema changes can break legacy persisted rows.
  - Mitigation: parse relation strings with compatibility aliases, persist canonical values, add regression tests.
- Risk: BFS/fusion changes can cause retrieval regressions.
  - Mitigation: bounded traversal, deterministic sorting, explicit scoring conformance tests.

## Interfaces / Contracts
- Relation payload contract remains string-based at tool boundary, but accepted values become canonical enum-backed relation labels.
- Search scoring contract shifts from additive graph-after-fusion to graph-in-fusion (RRF-included) while preserving score observability fields.
