# M91 - Spacebot G14 Adapter File Delivery Closure

- Milestone: https://github.com/njfio/Tau/milestone/91
- Epic: #2528
- Story: #2529
- Task: #2530
- Subtask: #2531

## Goal
Close the remaining G14 adapter wiring gap by delivering end-to-end send-file dispatch behavior across active transport adapters and validating with conformance, regression, mutation, and live checks.

## In Scope
- `tau-multi-channel` outbound file delivery for Discord and Telegram command paths.
- `tau-slack-runtime` send-file directive handling and Slack file upload dispatch.
- Conformance tests and runtime logs for success/failure reason codes.
- Checklist update in `tasks/spacebot-comparison.md`.

## Out of Scope
- New transport families or transport architecture redesign.
- Non-G14 roadmap items.
