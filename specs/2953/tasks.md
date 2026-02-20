# Tasks: Issue #2953 - Wire `/cortex/chat` to LLM with observer context and fallback

## Ordered Tasks
1. [x] T1 (RED): add failing tests for LLM-backed output and deterministic fallback.
2. [x] T2 (GREEN): replace mock output generator with LLM request + context builders.
3. [x] T3 (GREEN): preserve SSE event contract and add reason-coded metadata.
4. [x] T4 (REGRESSION): rerun cortex chat/status integration tests.
5. [x] T5 (VERIFY): run fmt, clippy, and scoped gateway tests; mark spec Implemented.

## Tier Mapping
- Unit: context builder and fallback behavior helpers.
- Property: N/A (bounded deterministic formatting).
- Contract/DbC: N/A (no contracts macro in this slice).
- Snapshot: N/A (explicit assertions preferred).
- Functional: `/cortex/chat` emits LLM-derived output.
- Conformance: C-01..C-04.
- Integration: authenticated SSE endpoint behavior.
- Fuzz: N/A (no parser surface change).
- Mutation: N/A (request/formatting glue path).
- Regression: cortex status/chat existing tests.
- Performance: N/A (bounded context; no benchmark harness change).
