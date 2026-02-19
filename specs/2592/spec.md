# Spec #2592 - Task: implement G6 relation enum + graph traversal parity improvements

Status: Implemented
Priority: P1
Milestone: M101
Parent: #2588

## Problem Statement
G6 parity remains incomplete because relation modeling still relies on free-form strings with a non-parity relation set, and search graph scoring is additive rather than graph-traversal-aware fusion.

## Scope
- Add a typed relation enum with Spacebot-parity six canonical relation types.
- Normalize relation inputs to canonical enum-backed values and persist/read them deterministically.
- Add bounded BFS traversal graph scoring seeded from initial ranked candidates.
- Fuse graph ranking into RRF scoring path (instead of post-RRF additive graph-only bump).
- Update G6 checklist parity lines after conformance validation.

## Out of Scope
- Dashboard graph visualization/API work.
- Memory lifecycle retention policy redesign.
- Non-memory tool interfaces unrelated to relation input/output.

## Acceptance Criteria
- AC-1: Memory runtime exposes a typed relation enum with exactly six canonical relation types (`related_to`, `updates`, `contradicts`, `caused_by`, `result_of`, `part_of`) and deterministic parse/serialize behavior.
- AC-2: Memory write/read/search relation paths store and return canonical relation values with fail-closed validation for unsupported relation types.
- AC-3: Memory search computes graph signals via bounded BFS traversal and fuses graph ranking into RRF scoring.
- AC-4: `tasks/spacebot-comparison.md` G6 parity bullets for relation enum, BFS traversal, and graph/RRF fusion reflect validated implementation status.

## Conformance Cases
- C-01 (AC-1, conformance): `cargo test -p tau-memory spec_2592_c01_memory_relation_enum_canonical_roundtrip -- --test-threads=1`
- C-02 (AC-2, conformance): `cargo test -p tau-memory spec_2592_c02_normalize_relations_accepts_only_supported_relation_enum_values -- --test-threads=1`
- C-03 (AC-2, functional): `cargo test -p tau-tools spec_2592_c03_memory_write_read_roundtrip_canonical_relation_type -- --test-threads=1`
- C-04 (AC-3, conformance): `cargo test -p tau-memory spec_2592_c04_search_graph_bfs_expands_two_hop_relation_paths -- --test-threads=1`
- C-05 (AC-3, regression): `cargo test -p tau-memory regression_2592_c05_search_fuses_graph_ranking_via_rrf_path -- --test-threads=1`
- C-06 (AC-4, process): G6 checklist bullets updated in `tasks/spacebot-comparison.md`

## Success Signals
- Relation data model is deterministic and parity-aligned across write/read/search flows.
- Graph traversal can surface relation-connected memories even when direct lexical/vector relevance is weak.
- Final ranking path uses fused vector/lexical/graph signals with deterministic ordering.
