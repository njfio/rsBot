# M12: P2 Memory Persistence

Milestone: `Gap List P2 Memory Persistence` (`#12`)

## Scope

Upgrade memory and persistence layers:

- real embedding integration
- hybrid BM25/vector retrieval
- structured persistence backends
- identity file system composition

## Active Spec-Driven Issues (current lane)

- `#1424` Epic: P2 Memory and Persistence Upgrade (3.1-3.4)
- `#1458` Story: 3.1 Real Vector Embedding Integration
- `#1460` Story: 3.2 Hybrid BM25 and Vector Retrieval
- `#1462` Story: 3.3 Structured Database Persistence Backends
- `#1464` Story: 3.4 Identity File System Composition

## Contract

Each implementation issue under this milestone must maintain:

- `specs/<issue-id>/spec.md`
- `specs/<issue-id>/plan.md`
- `specs/<issue-id>/tasks.md`

No implementation is considered complete until acceptance criteria are mapped to
conformance tests and verified in PR evidence.
