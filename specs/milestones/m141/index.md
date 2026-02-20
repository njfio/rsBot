# Milestone M141 - Tau Ops Dashboard PRD Phase 1M (Chat Message Send Visibility Contracts)

Status: InProgress

## Scope
Implement Tau Ops `/ops/chat` contracts for message-send visibility:
- deterministic chat send-form SSR markers,
- gateway chat transcript hydration from active session state,
- live `POST /ops/chat/send` behavior that appends user messages and returns to `/ops/chat` with preserved shell controls.

## Linked Issues
- Epic: #2828
- Story: #2829
- Task: #2830

## Success Signals
- `/ops/chat` SSR output includes deterministic send-form + transcript markers.
- Posting to `/ops/chat/send` appends the submitted user message to session state.
- Reloading `/ops/chat` shows the posted message in transcript markers.
- Existing Tau Ops shell contract suites remain green.
