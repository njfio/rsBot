# Plan: Issue #2969 - External coding-agent runtime extraction

## Approach
1. Identify all external coding-agent handlers and helper functions currently in `gateway_openresponses.rs`.
2. Create `gateway_openresponses/external_agent_runtime.rs` and move those functions with `pub(super)` visibility.
3. Import moved handlers/helpers in `gateway_openresponses.rs` and keep route constants/registrations unchanged.
4. Run targeted external coding-agent gateway tests plus formatting/lint checks.
5. Validate line-count reduction for `gateway_openresponses.rs`.

## Affected Paths
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/external_agent_runtime.rs` (new)
- `crates/tau-gateway/src/gateway_openresponses/tests.rs` (only if regression coverage addition is required)

## Risks and Mitigations
- Risk: subtle behavior drift during move.
  - Mitigation: no logic changes; move functions verbatim, keep signatures/routes intact, run targeted tests.
- Risk: visibility/import breakage.
  - Mitigation: use `pub(super)` functions and explicit imports; compile + test gate.

## Interfaces / Contracts
- external coding-agent endpoints:
  - `/gateway/external-coding-agent/sessions`
  - `/gateway/external-coding-agent/sessions/{session_id}`
  - `/gateway/external-coding-agent/sessions/{session_id}/progress`
  - `/gateway/external-coding-agent/sessions/{session_id}/followups`
  - `/gateway/external-coding-agent/sessions/{session_id}/followups/drain`
  - `/gateway/external-coding-agent/sessions/{session_id}/stream`
  - `/gateway/external-coding-agent/sessions/{session_id}/close`
  - `/gateway/external-coding-agent/reap`

## ADR
Not required (internal module boundary refactor, no architecture/policy contract change).
