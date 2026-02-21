# M170 - Gateway API Reference Documentation

Status: In Progress

## Objective
Publish a complete, operator/developer-facing API reference for gateway routes, including auth
requirements and policy-gate semantics.

## Scope
- Route inventory for OpenResponses, OpenAI-compatible, gateway control, dashboard, and cortex APIs.
- Endpoint-level auth expectation notes.
- Request/response example references for common operator workflows.

## Issues
- Epic: #2961
- Story: #2962
- Task: #2963

## Exit Criteria
- `docs/guides/gateway-api-reference.md` exists and is linked from `docs/README.md`.
- Reference route coverage is validated against `gateway_openresponses.rs` route table.
- Issue #2963 closes with conformance evidence.
