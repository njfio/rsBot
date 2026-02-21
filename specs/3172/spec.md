# Spec: Issue #3172 - training-proxy JSONL newline delimiter integrity

Status: Accepted

## Problem Statement
Attribution records are appended to `proxy-attribution.jsonl`. If an existing log ends without a trailing newline, the next appended record can be concatenated to the prior line, breaking JSONL record boundaries.

## Scope
In scope:
- Add conformance coverage for appending when the existing log lacks trailing newline.
- Implement minimal append behavior to preserve one-record-per-line JSONL boundaries.
- Retain existing successful append behavior for normal newline-terminated files.

Out of scope:
- Endpoint contract changes.
- Log rotation strategy changes.
- New dependencies.

## Acceptance Criteria
### AC-1 Append preserves JSONL boundaries when existing file lacks trailing newline
Given an existing attribution log file with one valid JSON record and no terminal newline,
when a new proxy attribution record is appended,
then the output file contains two separate newline-delimited JSON records.

### AC-2 Existing append behavior remains stable for normal files
Given a normal newline-terminated attribution log,
when a new record is appended,
then prior record content remains intact and exactly one new record is appended.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Integration/Conformance | existing attribution file with no trailing newline | proxy handles chat completion append | output contains two newline-delimited JSON lines |
| C-02 | AC-2 | Integration/Conformance | existing attribution file already newline-terminated | proxy handles chat completion append | output preserves prior line and appends one new line |

## Success Metrics / Observable Signals
- `cargo test -p tau-training-proxy spec_3172 -- --test-threads=1`
- `cargo test -p tau-training-proxy`
- `cargo fmt --check`
- `cargo clippy -p tau-training-proxy -- -D warnings`
