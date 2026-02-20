# Spec: Issue #2738 - G18 embedded dashboard shell route

Status: Accepted

## Problem Statement
Gateway exposes operational dashboard APIs and a webchat shell, but there is no dedicated embedded dashboard shell endpoint for SPA hosting evolution. This blocks completion of G18's `Serve embedded SPA from gateway` parity pathway.

## Acceptance Criteria

### AC-1 Gateway exposes embedded dashboard shell endpoint
Given gateway runtime is active,
When a client requests `/dashboard`,
Then gateway returns deterministic HTML shell content.

### AC-2 Embedded shell includes baseline navigation placeholders
Given `/dashboard` shell renders,
When operator inspects sections,
Then shell includes Overview, Sessions, Memory, and Configuration placeholder views with deterministic markers.

### AC-3 Gateway status advertises shell endpoint
Given authenticated status request,
When `/gateway/status` is requested,
Then payload includes `gateway.dashboard_shell_endpoint` with `/dashboard`.

### AC-4 Existing webchat/dashboard behavior remains compatible
Given prior `/webchat` and dashboard API flows,
When shell route is introduced,
Then existing functional/integration regressions remain green.

### AC-5 Scoped verification and live validation pass
Given this slice,
When validation runs,
Then `cargo fmt --check`, `cargo clippy -p tau-gateway -- -D warnings`, and `cargo test -p tau-gateway` pass, and live localhost route smoke confirms `/dashboard` shell availability.

## Scope

### In Scope
- New embedded dashboard shell renderer for gateway.
- New `/dashboard` route wiring in gateway router.
- Status payload endpoint advertisement for shell route.
- Unit/functional/integration regression coverage updates.
- G18 checklist evidence update in `tasks/spacebot-comparison.md`.

### Out of Scope
- Full React/Leptos dashboard implementation.
- New backend API schema changes.
- New dependency additions.

## Conformance Cases
- C-01 (unit): dashboard shell renderer contains deterministic route and section markers.
- C-02 (functional): `/dashboard` endpoint returns HTML shell response.
- C-03 (integration): `/gateway/status` advertises `dashboard_shell_endpoint`.
- C-04 (regression): existing `/webchat` and dashboard endpoint tests stay green.
- C-05 (verify/live): scoped gates and localhost route smoke pass.

## Success Metrics / Observable Signals
- Operators can load gateway-hosted dashboard shell at `/dashboard`.
- G18 `Serve embedded SPA from gateway` pathway item is closed with linked evidence.
- No regressions in current webchat/dashboard contract tests.
