# Milestone M150 - Tau Ops Dashboard PRD Phase 1V (Chat Tool-Result Card Contracts)

Status: InProgress

## Scope
Implement deterministic chat tool-result inline card contracts:
- explicit inline card marker elements for tool-role transcript rows,
- deterministic card marker attributes for selector-based validation,
- route-safe behavior across `/ops`, `/ops/chat`, and `/ops/sessions` while preserving existing contracts.

## Linked Issues
- Epic: #2864
- Story: #2865
- Task: #2866

## Success Signals
- Tool transcript rows render deterministic inline card markers.
- Non-tool transcript rows remain unchanged and do not emit tool-card markers.
- Existing chat/session visibility and token-counter contracts remain green.
