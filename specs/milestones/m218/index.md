# M218 - tau-training-proxy Malformed Attribution + Log Append Resilience

Status: In Progress

## Context
`tasks/tau-gaps-issues-improvements.md` still calls out quality depth gaps in `tau-training-proxy`, specifically malformed attribution handling and persistence/recovery behavior around attribution logs.

## Scope
- Add explicit conformance tests for malformed attribution header cases.
- Add resilience coverage for attribution log append behavior when storage path is missing and when the log already contains entries.
- Keep all changes scoped to `crates/tau-training-proxy` and its issue spec artifacts.

## Linked Issues
- Epic: #3162
- Story: #3163
- Task: #3164

## Success Signals
- `cargo test -p tau-training-proxy spec_3164 -- --test-threads=1`
- `cargo fmt --check`
- `cargo clippy -p tau-training-proxy -- -D warnings`
