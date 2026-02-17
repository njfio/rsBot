# M70 - G11 Slack Message Coalescing

Milestone objective: implement a production-safe first slice of G11 by adding configurable message coalescing to Slack bridge inbound processing.

## Scope
- Add configurable Slack coalescing window in milliseconds (default 2000ms).
- Batch rapid inbound Slack messages from the same user/thread into a single agent run.
- Preserve separation for messages outside the coalescing contract (different user/thread or outside window).
- Wire config through CLI -> onboarding transport config -> Slack runtime config.
- Add conformance/regression tests and verify quality gates.

## Out of Scope
- Coalescing for non-Slack transports in this milestone.
- Typing indicator support for coalescing window.
- Gateway webhook coalescing behavior changes.

## Exit Criteria
- Task `#2420` ACs implemented and verified.
- Coalescing logic and config are covered by conformance tests.
- Scoped verify gates pass (`fmt`, `clippy`, `tau-slack-runtime` + onboarding tests).
