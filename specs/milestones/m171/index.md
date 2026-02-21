# M171 - Gateway OpenResponses Module Split Phase 1

## Objective
Reduce `crates/tau-gateway/src/gateway_openresponses.rs` hotspot size by extracting one cohesive endpoint domain into dedicated submodule runtime files while preserving endpoint behavior.

## Scope
- Phase 1 extraction target: external coding-agent gateway endpoints (`/gateway/external-coding-agent/*`).
- Preserve route constants and route registration contracts.
- Preserve runtime behavior, auth/rate-limit semantics, and JSON/SSE response shapes.

## Linked Issues
- Epic: #2967
- Story: #2968
- Task: #2969

## Exit Criteria
- External agent handlers/helpers are moved out of `gateway_openresponses.rs`.
- `gateway_openresponses.rs` line count is reduced relative to baseline.
- Targeted gateway tests for external-agent routes pass.
