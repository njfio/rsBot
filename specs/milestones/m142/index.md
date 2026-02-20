# Milestone M142 - Tau Ops Dashboard PRD Phase 1N (Chat Session Selection Contracts)

Status: InProgress

## Scope
Implement Tau Ops `/ops/chat` active session selection contracts:
- deterministic session selector SSR markers on the chat route,
- deterministic selector option rows derived from discovered session files,
- active-session selection reflected in chat transcript and send-form state.

## Linked Issues
- Epic: #2832
- Story: #2833
- Task: #2834

## Success Signals
- `/ops/chat` HTML includes deterministic session selector container + option row markers.
- Selector options include discovered session keys and preserve an active-session selected state.
- Active session selection remains reflected in transcript rows and send-form hidden `session_key`.
- Existing Tau Ops shell marker suites remain green.
