# Tasks: Issue #2606 - Validate tau-gaps roadmap items and execute open P0/P1 remediations

## Ordered Tasks
1. T1 (RED): add failing expectations for stale roadmap/milestone status state.
2. T2 (GREEN): update roadmap and milestone docs to completed state for delivered tasks.
3. T3 (VERIFY): run docs quality checks for updated artifacts.
4. T4 (CLOSE): publish closeout evidence and transition #2606 to done.

## Tier Mapping
- Unit: N/A (docs/governance update)
- Property: N/A (no algorithmic surface)
- Contract/DbC: N/A (no contracts macro usage)
- Snapshot: N/A (explicit markdown assertions)
- Functional: C-01, C-02, C-03
- Conformance: C-01..C-04
- Integration: N/A (no runtime module integration)
- Fuzz: N/A (no untrusted parser)
- Mutation: N/A (docs-only scope)
- Regression: C-02 (milestone/status consistency)
- Performance: N/A (no hotspot/runtime change)
