# Issue 1629 Plan

Status: Reviewed

## Approach

1. Add tests-first conformance harness (`scripts/dev/test-memory-backend-disposition.sh`) that checks:
   - resolver only accepts `auto/jsonl/sqlite`
   - invalid backend fallback reason remains wired
   - memory ops runbook explicitly documents unsupported postgres behavior
2. Run harness expecting RED (docs note missing).
3. Update `docs/guides/memory-ops.md` with explicit unsupported-postgres statement.
4. Re-run harness for GREEN.
5. Run targeted regression test for AC-1:
   - `cargo test -p tau-memory regression_memory_store_treats_postgres_env_backend_as_invalid_and_falls_back`
6. Run scoped quality checks:
   - `cargo fmt --check`
   - `cargo clippy -p tau-memory -- -D warnings`

## Affected Areas

- `scripts/dev/test-memory-backend-disposition.sh`
- `docs/guides/memory-ops.md`
- `specs/1629/spec.md`
- `specs/1629/plan.md`
- `specs/1629/tasks.md`

## Risks And Mitigations

- Risk: docs drift from runtime behavior.
  - Mitigation: harness enforces explicit runbook wording and resolver invariants.
- Risk: accidental reintroduction of unsupported backend path.
  - Mitigation: keep regression test and conformance harness in CI-visible scripts.

## ADR

No dependency/protocol/architecture decision changes; ADR not required.
