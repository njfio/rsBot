# Plan: Issue #3000 - preflight-fast safety guard stage

## Approach
1. T1 RED: update `scripts/dev/test-preflight-fast.sh` to require guard-stage invocation and fail-closed behavior; run and capture failure.
2. Implement guard-stage wiring in `scripts/dev/preflight-fast.sh` with explicit ordering and log messages.
3. Run script regression tests and format gate.
4. Confirm conformance mapping C-01..C-05.

## Affected Modules
- `scripts/dev/preflight-fast.sh`
- `scripts/dev/test-preflight-fast.sh`
- `specs/milestones/m178/index.md`
- `specs/3000/{spec.md,plan.md,tasks.md}`

## Risks and Mitigations
- Risk: breaking existing fast-loop usage via changed invocation contract.
  - Mitigation: preserve passthrough semantics and keep command-line interface unchanged.
- Risk: preflight becomes slower/noisier.
  - Mitigation: reuse existing fast guard script and keep output concise.
- Risk: fail-closed logic accidentally still calls fast-validate.
  - Mitigation: explicit regression test assertions for failure branches.

## Interfaces / Contracts
- Entrypoint remains `scripts/dev/preflight-fast.sh [fast-validate args...]`.
- Guard script path configurable via env override:
  - `TAU_PANIC_UNSAFE_GUARD_BIN` (testability and deterministic stubbing).
