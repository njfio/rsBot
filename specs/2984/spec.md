# Spec: Issue #2984 - Split gateway config handlers into config_runtime module

Status: Reviewed

## Problem Statement
Gateway config endpoint handlers and override-policy helper plumbing are still implemented inline in `gateway_openresponses.rs`, increasing hotspot complexity and reducing maintainability.

## Acceptance Criteria

### AC-1 Config handlers are extracted into dedicated runtime module
Given gateway source layout,
When inspecting config endpoint implementation,
Then `GET /gateway/config` and `PATCH /gateway/config` handlers live in `gateway_openresponses/config_runtime.rs`.

### AC-2 Config helper plumbing is extracted with handlers
Given config override and runtime-heartbeat policy helper functions,
When extraction is complete,
Then tightly-coupled helper logic required by config handlers is colocated in `config_runtime.rs` and behavior remains unchanged.

### AC-3 Config endpoint behavior remains stable
Given existing config endpoint contracts,
When requests hit `/gateway/config`,
Then response payload fields, validation rules, restart/hot-reload semantics, and telemetry behavior remain unchanged.

### AC-4 Targeted config tests pass
Given gateway test suite,
When running config-targeted slices,
Then config endpoint and nearby regression tests remain green.

### AC-5 Hotspot size decreases again
Given baseline `gateway_openresponses.rs` line count,
When extraction is complete,
Then line count decreases from baseline.

## Scope

### In Scope
- Move config handlers and tightly-coupled helper functions into `config_runtime.rs`.
- Preserve route registration and endpoint constants.
- Run targeted regression and quality gates.

### Out of Scope
- config schema changes.
- auth/rate-limit policy changes.
- runtime heartbeat behavior changes.

## Conformance Cases
- C-01: `config_runtime.rs` contains extracted config handlers.
- C-02: required config helper plumbing moved and behavior preserved.
- C-03: targeted config endpoint tests pass.
- C-04: route bindings unchanged.
- C-05: `gateway_openresponses.rs` line count lower than baseline.

## Success Metrics / Observable Signals
- `cargo test -p tau-gateway config -- --test-threads=1` passes.
- `cargo test -p tau-gateway gateway_config -- --test-threads=1` passes.
- `cargo fmt --check` and `cargo clippy -p tau-gateway -- -D warnings` pass.
- `gateway_openresponses.rs` line count decreases from baseline.

## Approval Gate
P1 scope: spec authored/reviewed by agent; implementation proceeds and is flagged for human review in PR.
