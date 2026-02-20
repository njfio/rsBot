# Plan: Issue #2721 - Integrate ProcessManager/runtime profiles into live branch-worker execution

## Approach
1. Add RED tests for lineage registration, worker-profile enforcement, and supervisor terminal states on delegated execution.
2. Extend `Agent` runtime with process context fields and helper methods for profile application/snapshot retrieval.
3. Refactor `execute_branch_followup` to execute through supervised branch/worker process tasks, preserving existing fail-closed behavior.
4. Attach deterministic delegation metadata to branch result payload.
5. Run scoped verification and update G1 checklist entries covered by this slice.

## Affected Modules
- `crates/tau-agent-core/src/lib.rs`
- `crates/tau-agent-core/src/tests/process_architecture.rs`
- `crates/tau-agent-core/src/tests/config_and_direct_message.rs` (if payload-level assertions are needed)
- `tasks/spacebot-comparison.md`

## Risks / Mitigations
- Risk: introducing process metadata could break existing branch payload expectations.
  - Mitigation: append additive fields only; keep existing keys/reason codes unchanged.
- Risk: nested process spawning may leak tasks on error.
  - Mitigation: await supervised handles explicitly and propagate terminal state deterministically.
- Risk: profile enforcement could remove tools expected by branch regressions.
  - Mitigation: keep branch/worker allowlist behavior equivalent to existing memory-only enforcement and validate `spec_2602` tests.

## Interfaces / Contracts
- `Agent` gains process context/snapshot helpers for runtime supervision.
- `execute_branch_followup` now runs delegated work through `ProcessManager::spawn_supervised`.
- Branch result payload adds `process_delegation` object with channel/branch/worker metadata.

## ADR
- Not required. No new dependency family, protocol, or externally visible wire-format break; payload changes are additive.
