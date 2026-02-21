# Spec: Issue #2969 - Split external agent runtime handlers from gateway_openresponses

Status: Implemented

## Problem Statement
`crates/tau-gateway/src/gateway_openresponses.rs` remains a hotspot. The external coding-agent endpoint domain is implemented inline, increasing module size and review complexity. This phase extracts that domain into a dedicated runtime module without changing HTTP contracts.

## Acceptance Criteria

### AC-1 External coding-agent handlers and helper logic are extracted into a dedicated submodule
Given the gateway runtime source tree,
When reviewing external coding-agent endpoint implementation,
Then handler implementations and supporting helper logic live outside `gateway_openresponses.rs` in a dedicated module.

### AC-2 Route contracts stay stable
Given existing external coding-agent routes,
When requests are sent to `/gateway/external-coding-agent/*` endpoints,
Then endpoint paths, methods, auth behavior, and response semantics remain unchanged.

### AC-3 Regression tests for external coding-agent flow pass
Given the gateway test suite,
When running targeted external coding-agent tests,
Then existing behavior remains green.

### AC-4 Hotspot size is reduced
Given baseline module size,
When the refactor is complete,
Then `gateway_openresponses.rs` line count is lower than pre-change baseline.

## Scope

### In Scope
- extract external coding-agent handlers from `gateway_openresponses.rs`
- extract helper functions used only by those handlers
- wire module imports and route registrations
- run targeted regression tests

### Out of Scope
- endpoint schema or behavior changes
- auth/rate-limit policy changes
- broader domain splits beyond external coding-agent runtime

## Conformance Cases
- C-01: handlers for external coding-agent routes live in new runtime module.
- C-02: route registrations continue to point to handlers for all external coding-agent endpoints.
- C-03: external coding-agent tests pass post-refactor.
- C-04: `gateway_openresponses.rs` line count decreases from baseline.

## Success Metrics / Observable Signals
- `cargo test -p tau-gateway external_coding_agent -- --test-threads=1` passes.
- `gateway_openresponses.rs` line count reduced from baseline (3431).

## Approval Gate
P1 scope: spec authored/reviewed by agent; implementation proceeds and is flagged for human review in PR.
