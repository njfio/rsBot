# Plan: Issue #2730 - G18 stretch Cortex admin chat webchat panel

## Approach
1. Add RED tests asserting Cortex view markup and script stream-handler markers.
2. Implement Cortex tab/view UI controls in webchat HTML.
3. Add Cortex-specific request/stream handling functions reusing existing SSE parsing helpers.
4. Wire telemetry/status updates and keep existing tab behaviors unchanged.
5. Run scoped verification and update checklist/task artifacts.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses/webchat_page.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`
- `tasks/spacebot-comparison.md` (if checklist evidence is updated)

## Risks / Mitigations
- Risk: new UI wiring could regress existing tab initialization.
  - Mitigation: preserve existing DOM ids/listeners and add regression assertions.
- Risk: SSE parser logic could mix Cortex and conversation output streams.
  - Mitigation: isolate Cortex output/status sinks with explicit mode handling.

## Interfaces / Contracts
- Reuse backend contract `POST /cortex/chat` (SSE frames).
- Add frontend constant for cortex endpoint and DOM ids for cortex controls.

## ADR
- Not required: no new dependencies or protocol changes.
