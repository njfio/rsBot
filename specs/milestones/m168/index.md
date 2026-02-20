# Milestone M168 - Cortex Chat LLM Wiring and Operator Context

Status: InProgress

## Scope
Close the remaining Cortex reasoning gap by replacing mock `/cortex/chat` output with a real LLM-backed path that uses:
- observer event context,
- current Cortex bulletin snapshot,
- memory graph summary context,
- deterministic fallback output when provider calls fail.

## Linked Issues
- Epic: #2951
- Story: #2952
- Task: #2953

## Success Signals
- `/cortex/chat` calls provider client (`LlmClient::complete`) with bounded context.
- SSE contract remains stable (`created`, `delta`, `done`, `done` sentinel).
- Conformance tests verify LLM path and fallback path behavior.
