# Tasks: Issue #2667 - PRD memory explorer CRUD/search gateway endpoints

## Ordered Tasks
1. [x] T1 (RED): add failing integration tests for C-01..C-06.
2. [x] T2 (GREEN): add route templates and handler plumbing for entry-level memory CRUD endpoints.
3. [x] T3 (GREEN): implement search mode on `GET /gateway/memory/{session_key}` with scope/type filters.
4. [x] T4 (REGRESSION): ensure legacy memory blob + memory graph tests remain green.
5. [x] T5 (VERIFY): run scoped fmt/clippy/targeted tests and record evidence for C-07.

## Tier Mapping
- Unit: N/A (gateway behavior validated at integration boundary)
- Property: N/A (no randomized invariant surface in this slice)
- Contract/DbC: N/A (contracts macros not used in touched module)
- Snapshot: N/A (assertions are explicit JSON/status checks)
- Functional: C-02, C-04
- Conformance: C-01..C-07
- Integration: C-01..C-06
- Fuzz: N/A (no new parser over untrusted binary format)
- Mutation: N/A (incremental gateway endpoint extension, non-critical algorithm path)
- Regression: C-03, C-05, C-06
- Performance: N/A (no new performance budget contract in this slice)
