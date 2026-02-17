# M62 - Session Cost Tracking Completeness

Milestone objective: enforce and validate cumulative per-session token usage and estimated USD cost persistence across CLI runtime and gateway OpenResponses runtime.

## Scope
- Session usage/cost delta persistence correctness in `tau-session`.
- Runtime call-site coverage in `tau-coding-agent` and `tau-gateway`.
- Conformance evidence for accumulation + reload behavior.

## Out of Scope
- Provider pricing catalog redesign.
- Cross-session/global billing dashboards.
- Prompt caching and pre-flight token estimation (separate milestones).

## Exit Criteria
- AC/C-case mapping implemented for issue `#2376`.
- Conformance tests pass for session store, coding-agent runtime, and gateway runtime usage/cost accumulation.
- Mutation scoped to touched diff has no missed mutants.
