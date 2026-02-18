# Spec #2443 - add memory relation storage and graph-scored retrieval flow

Status: Implemented
Milestone: specs/milestones/m75/index.md
Issue: https://github.com/njfio/Tau/issues/2443

## Problem Statement

Tau required an end-to-end story slice for relation persistence and graph-aware
retrieval integration so that G6 phase-1 could be shipped through a single
cohesive delivery unit.

## Scope

In scope:

- Persist relation edges alongside runtime memory records.
- Surface relation descriptors in memory write/read/search flows.
- Integrate graph scoring signal into existing retrieval ranking flow.
- Preserve behavior for legacy relation-less records.

Out of scope:

- Relation inference automation.
- Lifecycle cleanup jobs from G7.
- Frontend graph visualization.

## Acceptance Criteria

- AC-1: Given valid relation inputs, when memories are written, then persisted
  relation edges are queryable from runtime storage.
- AC-2: Given read/search responses, when related records are returned, then
  relation metadata is present and stable.
- AC-3: Given otherwise-similar candidates, when one has stronger graph
  connectivity, then ranking includes graph signal contribution.
- AC-4: Given invalid relation payloads, when write is attempted, then request
  fails deterministically and no invalid edge is persisted.

## Conformance Cases

- C-01 (AC-1): `spec_2444_c01_memory_write_persists_relates_to_edges`.
- C-02 (AC-2): `spec_2444_c02_memory_search_includes_relation_metadata`.
- C-03 (AC-3): `spec_2444_c03_graph_signal_boosts_connected_candidate_ranking`.
- C-04 (AC-4): `spec_2444_c04_invalid_relation_payload_is_rejected_without_write`.
- C-05 (regression): `spec_2444_c05_legacy_records_without_relations_return_stable_defaults`.

## Success Metrics / Observable Signals

- PR #2446 merged with #2444 conformance suite passing.
- Runtime + tool crates pass fmt/clippy/tests and diff mutation gate.
