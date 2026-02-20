# M158 - Tau Ops Dashboard PRD Phase 2D (Chat Token Streaming Contracts)

## Context
Implements Tau Ops Dashboard PRD checklist item: "Agent response streams token-by-token" for `/ops/chat` SSR contracts.

## Linked Issues
- Epic: #2899
- Story: #2900
- Task: #2901

## Scope
- Deterministic assistant token-stream markers in chat transcript rows.
- Gateway + UI conformance tests proving token coverage and ordering.
- Regression safety for existing chat/session/dashboard contracts.

## Out of Scope
- Provider protocol/runtime changes.
- New dependencies.
- WebSocket protocol changes.
