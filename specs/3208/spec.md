# Spec: Issue #3208 - expand kamn-sdk browser DID init/report coverage

Status: Implemented

## Problem Statement
`kamn-sdk` has baseline DID init/report tests, but failure-path diagnostics are not explicitly enforced by spec coverage. Operator debugging needs deterministic request context for initialization failures, and report-write boundary failures need conformance protection.

## Scope
In scope:
- Add spec-derived conformance tests in `crates/kamn-sdk/src/lib.rs`.
- Ensure init failures include method/network/subject diagnostics while excluding entropy.
- Validate write failure behavior when output parent path is not a directory.

Out of scope:
- Changes to `kamn-core` identity algorithm behavior.
- API/schema changes to `BrowserDidInitReport`.
- New dependencies.

## Acceptance Criteria
### AC-1 init failures include actionable request context without leaking entropy
Given malformed DID initialization input,
when `initialize_browser_did` fails,
then the error includes method/network/subject context and excludes entropy values.

### AC-2 write failures at parent-path boundary are deterministic and contextual
Given `write_browser_did_init_report` is asked to write under a parent path that is a file,
when the write runs,
then it fails with contextual path diagnostics and no report file is written.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Conformance/Regression | malformed network + non-empty entropy | call `initialize_browser_did` | error includes method/network/subject and excludes entropy |
| C-02 | AC-2 | Conformance/Integration | parent path exists as file | call `write_browser_did_init_report` | error includes parent path context and nested report file absent |

## Success Metrics / Observable Signals
- `cargo test -p kamn-sdk`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
