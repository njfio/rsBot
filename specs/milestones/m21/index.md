# M21: Structural Runtime Hardening

Milestone: `Gap Closure Wave 2026-03: Structural Runtime Hardening` (`#21`)

## Scope

Close structural and operational gaps across runtime hardening workstreams:

- scaffold crate consolidation decisions and execution
- oversized production file decomposition
- safety mainline merge and validation
- roadmap hierarchy observability and control artifacts

## Active Spec-Driven Issues (current lane)

- `#1761` Generate machine-readable dependency graph for `#1678` hierarchy
- `#1767` Implement hierarchy graph extractor script (JSON + Markdown outputs)
- `#1768` Add graph artifact publication workflow and retention convention
- `#1769` Author critical-path update template with risk scoring rubric
- `#1770` Add critical-path update cadence policy and enforcement checklist

## Contract

Each implementation issue under this milestone must maintain:

- `specs/<issue-id>/spec.md`
- `specs/<issue-id>/plan.md`
- `specs/<issue-id>/tasks.md`

No implementation is considered complete until acceptance criteria are mapped to
conformance tests and verified in PR evidence.
