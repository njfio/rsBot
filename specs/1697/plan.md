# Issue 1697 Plan

Status: Reviewed

## Approach

1. Add tests-first conformance coverage in `tau-trainer` that expects:
   - deterministic reasoning/tool-use fixture family loading
   - scoring rubric normalization validation
   - explicit malformed-fixture error behavior
2. Add benchmark fixture data files and a README under
   `crates/tau-coding-agent/testdata/rl-benchmark-fixtures/`.
3. Implement a `benchmark_fixtures` module in `tau-trainer` with:
   - JSON fixture loading from file
   - structural validation and deterministic error messages
   - utility methods used by conformance tests
4. Run scoped fmt/clippy/tests and map ACs to conformance cases in PR evidence.

## Affected Areas

- `crates/tau-trainer/src/lib.rs`
- `crates/tau-trainer/src/benchmark_fixtures.rs` (new)
- `crates/tau-coding-agent/testdata/rl-benchmark-fixtures/README.md` (new)
- `crates/tau-coding-agent/testdata/rl-benchmark-fixtures/reasoning-suite.json` (new)
- `crates/tau-coding-agent/testdata/rl-benchmark-fixtures/tool-use-suite.json` (new)

## Risks And Mitigations

- Risk: fixture contract too rigid for future expansion.
  - Mitigation: enforce only baseline invariants (IDs, seeds, normalized rubric).
- Risk: non-deterministic parsing/ordering in tests.
  - Mitigation: sort case IDs and use deterministic weight assertions with tight
    epsilon.

## ADR

No new dependency or protocol/wire-format decision; ADR not required.
