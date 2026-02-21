# Spec: Issue #3028 - Publish crate dependency architecture diagram

Status: Implemented

## Problem Statement
The repository lacks a single, deterministic, published crate dependency architecture diagram. This gap slows onboarding and makes change-impact reasoning harder.

## Acceptance Criteria

### AC-1 Deterministic dependency graph script exists
Given repository workspace metadata,
When running the dependency graph script,
Then it emits deterministic JSON/Markdown artifacts with workspace crate and edge inventory.

### AC-2 Architecture documentation is published
Given `docs/architecture/`,
When reviewing architecture docs,
Then a crate dependency diagram doc exists and links the generation command and report artifacts.

### AC-3 Conformance tests validate script and doc contract
Given script/doc contracts,
When running conformance tests,
Then script schema/output and doc command markers pass.

### AC-4 Baseline checks remain green
Given all updates,
When running baseline checks,
Then `cargo fmt --check` and `cargo check -q` pass.

## Scope

### In Scope
- `scripts/dev/crate-dependency-graph.sh` (new)
- `scripts/dev/test-crate-dependency-graph.sh` (new)
- `docs/architecture/crate-dependency-diagram.md` (new)
- `tasks/reports/crate-dependency-graph.json` (generated)
- `tasks/reports/crate-dependency-graph.md` (generated)
- `specs/milestones/m185/index.md`
- `specs/3028/*`

### Out of Scope
- Crate/module refactors.
- Runtime behavior changes.
- CI workflow redesign.

## Conformance Cases
- C-01: Script succeeds with fixture metadata and emits expected schema/counts.
- C-02: Script succeeds against live workspace metadata and emits deterministic artifacts.
- C-03: Architecture doc contains command contract and artifact references.
- C-04: Baseline checks pass.

## Success Metrics / Observable Signals
- `bash scripts/dev/test-crate-dependency-graph.sh`
- `scripts/dev/crate-dependency-graph.sh --output-json tasks/reports/crate-dependency-graph.json --output-md tasks/reports/crate-dependency-graph.md --generated-at 2026-02-21T00:00:00Z`
- `cargo fmt --check`
- `cargo check -q`

## Approval Gate
P1 scope: spec authored/reviewed by agent; implementation proceeds and is flagged for human review in PR.
