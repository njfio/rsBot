# M220 - training-proxy JSONL Append Boundary Integrity

Status: In Progress

## Context
`tau-training-proxy` appends attribution records as JSONL. If the existing log file does not end in a newline, appending can collapse two records into a single line and violate JSONL boundary expectations.

## Scope
- Add conformance coverage for append behavior when existing file lacks trailing newline.
- Ensure append path preserves one-record-per-line JSONL contract.
- Keep changes scoped to `crates/tau-training-proxy` and issue spec artifacts.

## Linked Issues
- Epic: #3170
- Story: #3171
- Task: #3172

## Success Signals
- `cargo test -p tau-training-proxy spec_3172 -- --test-threads=1`
- `cargo test -p tau-training-proxy`
- `cargo fmt --check`
- `cargo clippy -p tau-training-proxy -- -D warnings`
