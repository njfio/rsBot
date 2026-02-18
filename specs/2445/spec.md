# Spec #2445 - RED/GREEN conformance coverage for G6 relation path

Status: Implemented
Milestone: specs/milestones/m75/index.md
Issue: https://github.com/njfio/Tau/issues/2445

## Problem Statement

G6 phase-1 required explicit test-first conformance coverage to prevent memory
relation and graph-scoring regressions during implementation merge.

## Scope

In scope:

- Define and deliver RED/GREEN conformance tests for G6 AC matrix.
- Add regression tests for legacy relation-less record compatibility.
- Add mutation-killing tests for graph scoring arithmetic branches.

Out of scope:

- Production feature additions beyond what #2444 required.
- New provider or UI functionality.

## Acceptance Criteria

- AC-1: Given spec conformance cases C-01..C-05, when RED phase runs before
  implementation, then tests fail for missing relation/graph behavior.
- AC-2: Given implementation completion, when GREEN phase runs, then C-01..C-05
  pass.
- AC-3: Given critical ranking and relation logic, when mutation gate executes
  on the issue diff, then there are zero missed mutants.

## Conformance Cases

- C-01 (AC-1/AC-2): `spec_2444_c01_memory_write_persists_relates_to_edges`.
- C-02 (AC-1/AC-2): `spec_2444_c02_memory_search_includes_relation_metadata`.
- C-03 (AC-1/AC-2): `spec_2444_c03_graph_signal_boosts_connected_candidate_ranking`.
- C-04 (AC-1/AC-2): `spec_2444_c04_invalid_relation_payload_is_rejected_without_write`.
- C-05 (AC-1/AC-2): `spec_2444_c05_legacy_records_without_relations_return_stable_defaults`.
- C-06 (AC-3): diff-scoped mutation run for `tau-memory` + `tau-tools`.

## Success Metrics / Observable Signals

- RED/GREEN evidence recorded in PR #2446.
- `cargo mutants --in-diff /tmp/issue2444.diff -p tau-memory -p tau-tools`:
  `55 caught, 14 unviable, 0 missed`.
