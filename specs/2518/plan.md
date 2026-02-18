# Plan #2518

Approach:
- Reuse existing reaction delivery path in `tau-multi-channel`; add the missing agent-facing tool contract and deterministic suppression semantics.
- Keep implementation narrow to avoid protocol churn.

Affected modules:
- `crates/tau-tools/src/tools.rs`
- `crates/tau-tools/src/tools/registry_core.rs`
- `crates/tau-tools/src/tools/tests.rs`
- `crates/tau-agent-core/src/lib.rs`
- `crates/tau-agent-core/src/tests/config_and_direct_message.rs`
- `crates/tau-coding-agent/src/events.rs`

Risks:
- Medium: false-positive suppression if react detection is too broad.

Mitigations:
- Restrict extraction to successful `tool_name == "react"` payloads with explicit directive fields.
- Add regression tests for non-react turns.
