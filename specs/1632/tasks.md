# Issue 1632 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): extend `runbook_ownership_docs_check.py` for dashboard/custom-command ownership requirements and run RED.

T2: update `docs/guides/dashboard-ops.md`, `docs/guides/custom-command-ops.md`, and `docs/guides/runbook-ownership-map.md` to satisfy ownership contract.

T3: run GREEN docs ownership check.

T4: run scoped roadmap/fmt/clippy checks and prepare PR evidence.

## Tier Mapping

- Functional: runbook/map ownership tokens present
- Regression: checker fails closed when required tokens are absent
- Integration: docs ownership check + scoped quality checks pass
