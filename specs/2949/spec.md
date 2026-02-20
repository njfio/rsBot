# Spec: Issue #2949 - Performance budget contract markers and conformance tests

Status: Implemented

## Problem Statement
Tau Ops PRD performance acceptance items (`2162-2165`) require explicit, testable contracts, but the current shell does not expose deterministic marker surfaces for budget thresholds (WASM size, LCP, layout shift mitigation, websocket processing latency).

## Acceptance Criteria

### AC-1 WASM bundle budget contract is declared
Given shell output,
When performance contracts are inspected,
Then output declares WASM budget threshold markers.

### AC-2 LCP budget contract is declared
Given shell output,
When performance contracts are inspected,
Then output declares LCP budget markers.

### AC-3 Layout-shift mitigation contract is declared
Given shell output,
When performance contracts are inspected,
Then output declares layout-shift budget and skeleton mitigation markers.

### AC-4 WebSocket processing budget contract is declared
Given shell output,
When performance contracts are inspected,
Then output declares websocket message processing latency markers.

## Scope

### In Scope
- Add deterministic performance contract markers to `tau-dashboard-ui` SSR output.
- Add conformance tests for PRD items `2162-2165`.
- Keep existing shell behavior stable.

### Out of Scope
- Runtime performance benchmarking in CI.
- WASM bundling pipeline changes.
- Browser performance telemetry ingestion.

## Conformance Cases
- C-01: WASM budget marker exists (`<500KB gzipped` contract token).
- C-02: LCP budget marker exists (`<1.5s` contract token).
- C-03: layout-shift and skeleton mitigation markers exist.
- C-04: websocket processing budget marker exists (`<50ms` contract token).

## Success Metrics / Observable Signals
- `cargo test -p tau-dashboard-ui -- --test-threads=1` passes with new performance contract tests.
- Existing dashboard contract suites remain green.

## Approval Gate
P1 single-module scope. Spec is agent-reviewed and proceeds under explicit user instruction to continue the contract process.
