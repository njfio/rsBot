# Spec: Issue #2742 - G18 priority pages baseline in embedded dashboard shell

Status: Accepted

## Problem Statement
The embedded `/dashboard` shell exists but still renders static placeholders. G18 priority pages require baseline functional views for Overview, Sessions, Memory, and Configuration so operators can inspect runtime state from one dashboard shell.

## Acceptance Criteria

### AC-1 Overview page loads dashboard health/widget summaries
Given `/dashboard` is loaded,
When overview refresh is triggered,
Then shell fetches dashboard health/widgets endpoints and renders deterministic summary output.

### AC-2 Sessions page loads session list and selected detail
Given valid auth context,
When sessions refresh and detail load are triggered,
Then shell fetches `/gateway/sessions` and `/gateway/sessions/{session_key}` and renders deterministic outputs.

### AC-3 Memory page loads graph summary diagnostics
Given valid auth context,
When memory refresh is triggered,
Then shell fetches `/api/memories/graph` and renders deterministic node/edge summary output.

### AC-4 Configuration page loads gateway config summary
Given valid auth context,
When configuration refresh is triggered,
Then shell fetches `/gateway/config` and renders deterministic config summary output (read-only baseline).

### AC-5 Existing behavior remains compatible and verification gates pass
Given current gateway/webchat/dashboard flows,
When priority-page wiring is added,
Then existing regressions remain green and scoped gates + live localhost smoke pass.

## Scope

### In Scope
- `/dashboard` shell JS wiring for overview/sessions/memory/configuration fetch flows.
- Deterministic page-level status/output render blocks and controls.
- Unit/functional tests for dashboard shell markers/handlers.
- G18 checklist evidence update in `tasks/spacebot-comparison.md`.

### Out of Scope
- Full design-system quality SPA rewrite.
- Write/edit operations for configuration (read-only baseline only).
- Backend API schema changes.

## Conformance Cases
- C-01 (unit): dashboard shell includes controls/markers for overview/sessions/memory/configuration API views.
- C-02 (functional): `/dashboard` endpoint serves updated shell with priority-page controls.
- C-03 (integration): existing endpoint contracts remain compatible while dashboard shell gains API wiring.
- C-04 (regression): existing `/webchat` and dashboard route tests remain green.
- C-05 (verify/live): fmt/clippy/tau-gateway tests and localhost `/dashboard` live smoke pass.

## Success Metrics / Observable Signals
- Operators can inspect overview/session/memory/configuration baseline payloads directly in `/dashboard`.
- G18 priority pages checklist item is closed with linked issue/PR evidence.
- No regressions in gateway UI/runtime test suite.
