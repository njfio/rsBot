# Plan #2520

Approach:
- Introduce `ReactTool` in `tau-tools` with strict arg parsing.
- Add `extract_react_response` helper in `tau-agent-core` and set suppression flag on successful react tool results.
- Extend `tau-coding-agent` event payload builder to optionally attach reaction metadata and reason code.

Affected modules:
- `crates/tau-tools/src/tools.rs`
- `crates/tau-tools/src/tools/registry_core.rs`
- `crates/tau-tools/src/tools/tests.rs`
- `crates/tau-agent-core/src/lib.rs`
- `crates/tau-agent-core/src/tests/config_and_direct_message.rs`
- `crates/tau-coding-agent/src/events.rs`

Risks:
- Medium: suppression could trigger on malformed/non-react payloads.

Mitigations:
- Parse only valid successful react payloads.
- Add regression tests asserting no suppression for normal responses.
