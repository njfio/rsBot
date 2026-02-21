# Spec: Issue #2975 - Split gateway session handlers into session API runtime module

Status: Reviewed

## Problem Statement
`crates/tau-gateway/src/gateway_openresponses.rs` remains above desired hotspot size after phase 1 extraction. Session endpoint handlers are still implemented inline and can be cleanly extracted into a dedicated runtime module.

## Acceptance Criteria

### AC-1 Session endpoint handlers are extracted into a dedicated module
Given the gateway source tree,
When inspecting session endpoint implementation,
Then session handlers live in `gateway_openresponses/session_api_runtime.rs` instead of `gateway_openresponses.rs`.

### AC-2 Session route behavior remains stable
Given existing session endpoints,
When requests are sent to `/gateway/sessions*` routes,
Then auth, policy-gate checks, and response semantics are unchanged.

### AC-3 Targeted session regression tests pass
Given the gateway test suite,
When running session endpoint tests,
Then targeted session integration/functional tests remain green.

### AC-4 Hotspot size is reduced below phase-2 threshold
Given baseline `gateway_openresponses.rs` line count,
When extraction is complete,
Then the file line count is below 2800.

## Scope

### In Scope
- extract session handlers from `gateway_openresponses.rs`
- extract local helper functions tightly coupled to moved handlers
- preserve route wiring and constants
- run targeted regression checks

### Out of Scope
- endpoint schema changes
- auth policy changes
- memory/safety/deploy endpoint refactors

## Conformance Cases
- C-01: `session_api_runtime.rs` contains extracted session handlers.
- C-02: route table still binds session paths to correct handlers.
- C-03: targeted session tests pass.
- C-04: `gateway_openresponses.rs` line count < 2800.

## Success Metrics / Observable Signals
- `cargo test -p tau-gateway gateway_session -- --test-threads=1` passes.
- `cargo test -p tau-gateway sessions -- --test-threads=1` passes.
- `gateway_openresponses.rs` line count below 2800.

## Approval Gate
P1 scope: spec authored/reviewed by agent; implementation proceeds and is flagged for human review in PR.
