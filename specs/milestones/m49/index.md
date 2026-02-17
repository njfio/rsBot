# M49 — Gateway Token Preflight Enforcement

Milestone: [GitHub milestone #49](https://github.com/njfio/Tau/milestone/49)

## Objective

Ensure gateway OpenResponses requests enforce preflight token ceilings before provider dispatch so oversized prompts fail fast and predictably.

## Scope

- Wire `AgentConfig` preflight token ceilings for gateway OpenResponses runtime.
- Derive token ceilings from existing gateway input-char limits (no new provider/model-catalog dependency in gateway crate).
- Add conformance tests for fail-fast budget enforcement and non-regression for valid requests.

## Out of Scope

- Provider/model-catalog dynamic context-window lookup in gateway.
- Prompt truncation logic.
- Changes to OpenResponses response schema.

## Linked Hierarchy

- Epic: #2307
- Story: #2308
- Task: #2309
- Subtask: #2310
