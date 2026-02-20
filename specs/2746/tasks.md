# Tasks: Issue #2746 - G18 dashboard architecture/stack decision ADR closure

## Ordered Tasks
1. [x] T1 (RED): identify unresolved G18 decision rows and document expected closure artifacts.
2. [x] T2 (GREEN): author ADR-006 with architecture location + stack selection decision.
3. [x] T3 (GREEN): update `tasks/spacebot-comparison.md` decision/stack checklist rows with evidence.
4. [x] T4 (REGRESSION): verify touched docs are coherent with existing dashboard implementation path.
5. [x] T5 (VERIFY): run `cargo fmt --check` and `cargo clippy -p tau-gateway -- -D warnings`.

## Tier Mapping
- Unit: N/A (docs-only scope)
- Property: N/A (docs-only scope)
- Contract/DbC: N/A (docs-only scope)
- Snapshot: N/A (docs-only scope)
- Functional: N/A (docs-only scope)
- Conformance: C-01..C-04
- Integration: N/A (docs-only scope)
- Fuzz: N/A (docs-only scope)
- Mutation: N/A (docs-only scope)
- Regression: C-03
- Performance: N/A (docs-only scope)
