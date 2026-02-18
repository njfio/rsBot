# Spec #2444 - G6 memory graph relations (phase 1)

Status: Accepted
Milestone: specs/milestones/m75/index.md
Issue: https://github.com/njfio/Tau/issues/2444

## Problem Statement

Tau memory currently stores isolated records (even with G5 type/importance), but
cannot persist relation edges or use graph proximity as a ranking signal. This
leaves `tasks/spacebot-comparison.md` gap `G6` incomplete.

## Scope

In scope:

- Persist relation edges (`source_id`, `target_id`, `relation_type`, `weight`).
- Allow relation attachment during memory write operations.
- Return relation metadata in read/search payloads.
- Add graph-signal contribution to memory search ranking (3-way blend with
  existing lexical/vector scoring).

Out of scope:

- Automated relation inference by LLM.
- Decay/prune/orphan lifecycle jobs (`G7`).
- Frontend graph visualization (`G19`).

## Acceptance Criteria

- AC-1: Given a valid relation payload on memory write, when the write
  completes, then the relation edge is persisted and queryable.
- AC-2: Given read/search requests for related memories, when records are
  returned, then relation metadata includes `target_id`, `relation_type`, and
  effective weight.
- AC-3: Given search candidates with equal lexical/vector scores, when one has
  stronger graph connectivity from related high-importance nodes, then that
  candidate ranks higher in final ordering.
- AC-4: Given invalid relation inputs (unknown target, invalid relation type,
  out-of-range weight), when write is requested, then tool returns deterministic
  validation error and no relation edge is persisted.
- AC-5: Given legacy records without relation rows, when read/search executes,
  then operations succeed with no parse/migration failure and stable defaults.

## Conformance Cases

- C-01 (AC-1, conformance/functional): write memory with `relates_to` persists
  relation edge and returns edge metadata.
- C-02 (AC-2, conformance/integration): read/search payload includes stored
  relation descriptors for related records.
- C-03 (AC-3, conformance/integration): graph-connected record outranks
  otherwise-equal unconnected record in final ranking.
- C-04 (AC-4, conformance/unit): invalid relation payload returns stable
  `memory_invalid_relation` reason and writes nothing.
- C-05 (AC-5, regression): legacy relation-less fixtures continue to
  read/search successfully.

## Success Metrics / Observable Signals

- C-01..C-05 tests pass.
- Existing `tau-memory` and `tau-tools` memory tests remain green.
- No migration failure for existing local SQLite memory stores.
