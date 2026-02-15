# Issue 1666 Plan

Status: Reviewed

## Approach

1. Verify collector burst/no-drop behavior using existing load harness test.
2. Verify retry/requeue behavior with stale worker chaos tests.
3. Capture conformance mapping in this parent task spec and close via evidence.

## Affected Areas

- `specs/1666/{spec,plan,tasks}.md`
- verification-only run against:
  - `crates/tau-training-runner/src/lib.rs`
  - `crates/tau-training-store/src/lib.rs`
  - `crates/tau-training-store/src/sqlite.rs`
  - `scripts/dev/collector-load-harness.sh`

## Risks And Mitigations

- Risk: parent task closure drifts from child implementation state.
  - Mitigation: bind ACs to concrete existing conformance tests and rerun them.
- Risk: relying on historical behavior without fresh verification.
  - Mitigation: execute scoped tests and harness in this PR evidence.

## ADR

No architecture/dependency/protocol change. ADR not required.
