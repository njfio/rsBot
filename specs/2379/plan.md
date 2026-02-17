# Plan: Issue #2379

## Approach
1. Define a canonical QA-loop config dedicated to session-cost mutation validation.
2. Add conformance tests in `tau-ops` that:
   - load/validate the config and enforce scoped commands;
   - verify fail-fast semantics on first failing stage;
   - verify documentation includes canonical invocation contract.
3. Add operator docs for deterministic invocation and env overrides.
4. Run scoped quality gates for the touched crate and targeted tests.

## Affected Modules
- `crates/tau-ops/src/qa_loop_commands.rs`
- `docs/qa/session-cost-mutation.qa-loop.json`
- `docs/qa/session-cost-mutation-lane.md`

## Risks and Mitigations
- Risk: command text drift between docs and config.
  Mitigation: conformance test checks doc contains canonical command references.
- Risk: lane drifts into broad workspace scope again.
  Mitigation: conformance test enforces package/file/test scoping markers in mutation stages.

## Interfaces / Contracts
- QA-loop config remains schema version `1` and uses existing `QaLoopConfigFile` contract.
- Mutation stages must use scoped `cargo mutants` commands with explicit package/file/test targeting and `--baseline skip`.

## ADR
No ADR required; no architecture/protocol/dependency change.
