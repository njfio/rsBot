# Plan #2514

Approach:
- Add a small helper in `crates/tau-coding-agent/src/events.rs` to build outbound payloads.
- If skip reason exists in current-turn messages, include `skip_reason` and stable `reason_code` in outbound log payload.
- Add focused conformance unit tests in the same module.

Affected modules:
- `crates/tau-coding-agent/src/events.rs`
- `tasks/spacebot-comparison.md`

Risks:
- Low: payload-shape drift for existing consumers.

Mitigations:
- Preserve existing fields and only append optional skip diagnostics.
