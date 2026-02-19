# Spec #2593 - Subtask: package conformance + live validation evidence for G6 relation/traversal rollout

Status: Implemented
Priority: P1
Milestone: M101
Parent: #2592

## Problem Statement
#2592 introduces relation model and search ranking behavior changes that require reproducible verification and live validation evidence before merge.

## Scope
- Re-run #2592 conformance suite.
- Run scoped quality gates + mutation-in-diff.
- Run sanitized live smoke and capture summary.
- Update process/closure artifacts.

## Out of Scope
- New feature work beyond #2592 acceptance criteria.

## Acceptance Criteria
- AC-1: #2592 conformance cases are reproducibly covered and green.
- AC-2: mutation-in-diff reports zero missed mutants for #2592 Rust changes.
- AC-3: sanitized live smoke reports `failed=0`.
- AC-4: issue logs and spec/task closure artifacts are complete.

## Conformance Cases
- C-01 (AC-1, conformance): mapped #2592 commands pass.
- C-02 (AC-2, mutation): `cargo mutants --in-diff <issue2592-diff>`.
- C-03 (AC-3, live validation): `./scripts/dev/provider-live-smoke.sh` summary reports `failed=0`.
- C-04 (AC-4, process): closure comments + status/spec updates are present.
