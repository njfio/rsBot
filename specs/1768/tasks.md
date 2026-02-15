# Issue 1768 Tasks

Status: Implementing

## Ordered Tasks

T1 (tests-first): add publication contract tests for naming, index generation,
and retention pruning.

T2: add publication policy JSON with naming + retention contract.

T3: implement `hierarchy-graph-publish.sh` and index/prune behavior.

T4: document extract + publish + retention workflow in roadmap operator docs.

T5: run targeted and regression test matrix; capture evidence in issue/PR.

## Tier Mapping

- Unit: policy/schema and script-existence assertions
- Functional: publish execution and index creation from valid inputs
- Integration: repeated publication and discoverability index stability
- Regression: retention pruning and docs-link contract checks
- Conformance: C-01..C-05 mappings validated in contract tests
