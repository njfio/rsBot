# Spec: Issue #2988 - Split channel lifecycle and UI telemetry handlers into channel_telemetry_runtime module

Status: Implemented

## Problem Statement
`gateway_openresponses.rs` still embeds channel lifecycle and UI telemetry handler logic, expanding hotspot size and mixing concerns.

## Acceptance Criteria

### AC-1 Channel lifecycle and UI telemetry handlers are extracted
Given gateway source layout,
When inspecting runtime handlers,
Then channel lifecycle and UI telemetry handlers live in `gateway_openresponses/channel_telemetry_runtime.rs`.

### AC-2 Tightly-coupled helper plumbing is extracted with handlers
Given helpers used only by these handlers,
When extraction is complete,
Then helper plumbing is colocated in the new module and behavior remains unchanged.

### AC-3 Endpoint behavior remains stable
Given existing `/gateway/channels/{channel}/lifecycle` and `/gateway/ui/telemetry` endpoints,
When requests are handled,
Then auth checks, request validation, persistence behavior, and response payloads remain unchanged.

### AC-4 Targeted tests remain green
Given existing channel lifecycle and UI telemetry tests,
When scoped suites run,
Then they pass without regression.

### AC-5 Hotspot size drops below 2000 lines
Given baseline `gateway_openresponses.rs` line count,
When extraction completes,
Then file line count is below 2000.

## Scope

### In Scope
- Move channel lifecycle and UI telemetry handlers into dedicated module.
- Move helper functions used only by moved handlers.
- Keep route constants and route registration unchanged.
- Run scoped regression and quality gates.

### Out of Scope
- Endpoint schema changes.
- auth/rate-limit policy changes.
- lifecycle command semantics changes.

## Conformance Cases
- C-01: new module contains extracted handlers.
- C-02: helper plumbing moved and behavior preserved.
- C-03: scoped channel lifecycle/telemetry tests pass.
- C-04: route bindings unchanged.
- C-05: `gateway_openresponses.rs` line count < 2000.

## Success Metrics / Observable Signals
- `cargo test -p tau-gateway channel_lifecycle -- --test-threads=1` passes.
- `cargo test -p tau-gateway ui_telemetry -- --test-threads=1` passes.
- `cargo fmt --check` and `cargo clippy -p tau-gateway -- -D warnings` pass.
- `gateway_openresponses.rs` line count below 2000.

## Approval Gate
P1 scope: spec authored/reviewed by agent; implementation proceeds and is flagged for human review in PR.
