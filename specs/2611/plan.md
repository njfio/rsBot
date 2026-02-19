# Plan: Issue #2611 - Provider outbound token bucket

## Approach
1. Add CLI flag coverage tests first for new rate-limit knobs.
2. Add provider client tests for limiter delay/fail-closed semantics (red).
3. Implement an async token-bucket limiter wrapper around `LlmClient` in `tau-provider`.
4. Wire wrapper in `build_provider_client` for HTTP provider routes.
5. Run scoped verification and publish issue evidence.

## Affected Modules
- `crates/tau-cli/src/cli_args.rs`
- `crates/tau-coding-agent/src/tests/cli_validation.rs`
- `crates/tau-provider/src/client.rs`
- `specs/2611/spec.md`
- `specs/2611/plan.md`
- `specs/2611/tasks.md`
- `specs/milestones/m104/index.md`

## Risks / Mitigations
- Risk: limiter introduces excessive latency under burst load.
  - Mitigation: configurable `max_wait_ms` fail-closed guard.
- Risk: limiter wrapper breaks existing provider client semantics.
  - Mitigation: integration tests with deterministic mock client wrappers.
- Risk: concurrent calls produce non-deterministic token accounting.
  - Mitigation: mutex-protected token state and deterministic refill math.

## Interfaces / Contracts
- New CLI knobs:
  - `--provider-rate-limit-capacity`
  - `--provider-rate-limit-refill-per-second`
  - `--provider-rate-limit-max-wait-ms`
- `build_provider_client` contract:
  - Default behavior unchanged when limiter disabled.
  - Returns throttled wrapper client when limiter enabled.

## ADR
- Not required: no new dependencies or architecture/protocol changes.
