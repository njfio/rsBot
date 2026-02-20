# Plan: Issue #2758 - Discord polling message history backfill (up to 100 before trigger) (G10)

## Approach
1. Add RED tests for first-run backfill request limit (`100`), incremental cursor behavior, and guild-filter compatibility.
2. Introduce explicit Discord first-run backfill limit constant and request-limit selection in polling loop.
3. Preserve existing ordering/cursor update semantics while switching limit based on cursor presence.
4. Run scoped gates and a local live poll-once validation run.
5. Update `tasks/spacebot-comparison.md` with issue evidence.

## Affected Modules
- `crates/tau-multi-channel/src/multi_channel_live_connectors.rs`
- `tasks/spacebot-comparison.md`

## Risks / Mitigations
- Risk: cursor handling regression could replay old messages.
  - Mitigation: add explicit regression test proving second run only ingests newer IDs.
- Risk: larger first-run batch could bypass policy filters.
  - Mitigation: test guild allowlist enforcement on first-run backfill path.

## Interfaces / Contracts
- Internal behavior contract only:
  - No-cursor Discord channel polling uses `limit=100` for first-run backfill.
  - Cursor-present polling keeps incremental limit semantics.
- External CLI/API contract remains unchanged.

## ADR
- Not required: no dependency, protocol, or architecture change.
