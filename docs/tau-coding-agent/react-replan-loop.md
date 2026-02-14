# ReAct Replan Loop (Issue #1187)

Tau now supports bounded replanning when a tool turn fully fails and the assistant reports failure instead of continuing execution.

## Runtime behavior

- Config:
  - `AgentConfig.react_max_replans_on_tool_failure` (default `1`)
- Trigger conditions:
  - previous turn had tool calls and all tool results were errors
  - current assistant response has no tool calls
  - assistant text is empty or contains failure markers (`cannot`, `unable`, `failed`, `error`, etc.)
- Action:
  - emits `AgentEvent::ReplanTriggered { turn, reason }`
  - appends a user replan instruction asking the model to choose an alternative tool path
  - continues the loop up to the configured replan budget

## Observability

- `AgentEvent::ReplanTriggered` is serialized by runtime event JSON as:
  - `type: "replan_triggered"`
  - `turn`
  - `reason`

## Safety model

- Default behavior is bounded (`1`) to avoid runaway loops.
- Set `react_max_replans_on_tool_failure` to `0` to disable replanning.
