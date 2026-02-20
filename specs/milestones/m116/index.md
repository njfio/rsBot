# M116 - Spacebot G18 Stretch Cortex Admin Chat UI

## Context
`tasks/spacebot-comparison.md` still tracks unresolved G18 stretch-page parity for a Cortex admin chat operator surface. Tau already exposes authenticated Cortex runtime endpoints (`POST /cortex/chat`, `GET /cortex/status`) but the gateway webchat does not provide an operator panel for interactive Cortex prompt streaming.

## Linked Work
- Epic: #2728
- Story: #2729
- Task: #2730
- Source parity checklist: `tasks/spacebot-comparison.md` (G18 stretch page)

## Scope
- Add a Cortex admin view in gateway webchat.
- Wire authenticated SSE stream handling for `/cortex/chat`.
- Preserve existing webchat dashboards/sessions/memory/config behavior.

## Exit Criteria
- Webchat renders dedicated Cortex admin controls and output panes.
- Cortex prompts can be submitted from webchat and stream events are rendered deterministically.
- Regression tests for existing webchat/memory/dashboard behaviors stay green.
