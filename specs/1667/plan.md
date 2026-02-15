# Issue 1667 Plan

Status: Reviewed

## Approach

1. Add tests that encode both stale-timeout and healthy-heartbeat behavior.
2. Update store backends so worker heartbeat refresh also updates active running
   attempt heartbeat timestamps.
3. Verify runner behavior under concurrent reassignment to prevent false
   timeouts while preserving stale-attempt recovery.

## Affected Areas

- `crates/tau-training-store/src/lib.rs`
- `crates/tau-training-store/src/sqlite.rs`
- `crates/tau-training-runner/src/lib.rs`
- `specs/1667/{spec,plan,tasks}.md`

## Risks And Mitigations

- Risk: heartbeat updates could mutate non-running attempts.
  - Mitigation: only refresh attempt heartbeat when attempt is in `Running`.
- Risk: backend semantic drift between in-memory and sqlite paths.
  - Mitigation: add mirrored conformance tests in both backends.

## ADR

No architecture/dependency/protocol change. ADR not required.
