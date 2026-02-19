# Tasks: Issue #2617 - Memory graph visualization API and dashboard view

## Ordered Tasks
1. T1 (RED): add failing tests for memory-graph endpoint + webchat graph controls.
2. T2 (GREEN): implement memory graph endpoint with deterministic extraction and filter handling.
3. T3 (GREEN): implement memory-tab graph controls and SVG rendering cues.
4. T4 (VERIFY): run scoped fmt/clippy/tests and confirm AC/C mapping.
5. T5 (CLOSE): update issue process log and open PR with tier matrix + TDD evidence.

## Tier Mapping
- Unit: C-01
- Property: N/A (no property harness required for this deterministic extraction slice)
- Contract/DbC: N/A (no new contracts annotations)
- Snapshot: N/A (structured assertions preferred)
- Functional: C-02
- Conformance: C-01..C-05
- Integration: C-04
- Fuzz: N/A (no new parser over untrusted binary formats)
- Mutation: N/A (UI/export plumbing slice with deterministic functional+regression coverage)
- Regression: C-03
- Performance: N/A (no hotspot contract changes)
