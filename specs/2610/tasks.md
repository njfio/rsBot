# Tasks: Issue #2610 - Stale merged remote branch hygiene

## Ordered Tasks
1. T1 (RED): add fixture-based script tests for dry-run inventory output, delete guardrails, and explicit prune behavior.
2. T2 (GREEN): implement stale merged branch prune script with dry-run default and explicit delete confirmation flags.
3. T3 (GREEN): add deterministic JSON + Markdown report generation for inventory/audit evidence.
4. T4 (GREEN): update stale-branch playbook with rollback steps using recorded branch SHA.
5. T5 (VERIFY): run script contract tests and scoped formatting/lint checks.
6. T6 (CLOSE): update issue process log and readiness evidence.

## Tier Mapping
- Unit: N/A (shell script contract tests cover behavior end-to-end)
- Property: N/A (no randomized invariant surface)
- Contract/DbC: N/A (no Rust public API changes)
- Snapshot: N/A (assert deterministic report fields directly)
- Functional: C-01
- Conformance: C-01..C-03
- Integration: C-03
- Fuzz: N/A (no parser exposed to untrusted free-form input beyond git refs)
- Mutation: N/A (script-level scoped slice)
- Regression: C-02
- Performance: N/A (operator-run maintenance utility)
