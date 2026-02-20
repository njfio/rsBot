# Tasks: Issue #2758 - Discord polling message history backfill (up to 100 before trigger) (G10)

## Ordered Tasks
1. [x] T1 (RED): add failing tests for first-run `limit=100` backfill and cursored incremental behavior.
2. [x] T2 (GREEN): implement first-run request-limit selection and preserve cursor updates.
3. [x] T3 (GREEN): validate guild allowlist filtering remains enforced during backfill.
4. [x] T4 (REGRESSION): verify second-run behavior only ingests new Discord messages.
5. [x] T5 (VERIFY): run fmt, clippy, targeted tests, and local live validation.
6. [x] T6 (DOC): update `tasks/spacebot-comparison.md` for G10 backfill evidence.

## Tier Mapping
- Unit: N/A (behavior covered at functional/integration boundary)
- Property: N/A (no randomized invariant surface)
- Contract/DbC: N/A (no contract macro changes)
- Snapshot: N/A (assertive behavior tests)
- Functional: C-01, C-03
- Conformance: C-01..C-05
- Integration: C-02
- Fuzz: N/A (no new parser boundary)
- Mutation: N/A (non-critical path delta with strong conformance tests)
- Regression: C-02
- Performance: N/A (no hotspot benchmarked)
