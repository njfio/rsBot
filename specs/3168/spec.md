# Spec: Issue #3168 - kamn-core label boundary validation hardening

Status: Implemented

## Problem Statement
`kamn-core` currently allows identifier labels that begin or end with `-` or `_`. This weakens input boundary guarantees for network/subject identity values used in DID generation and auth-adjacent workflows.

## Scope
In scope:
- Reject network/subject labels with leading or trailing non-alphanumeric boundary characters (`-`, `_`).
- Add conformance tests proving malformed boundary inputs fail deterministically.
- Preserve existing normalization/determinism behavior for valid identifiers.

Out of scope:
- DID method expansion or wire/protocol changes.
- New dependencies.
- Multi-crate refactors.

## Acceptance Criteria
### AC-1 Malformed label boundaries are rejected
Given `BrowserDidIdentityRequest` identifiers,
when any dot-separated label starts or ends with a non-alphanumeric boundary marker,
then `build_browser_did_identity` fails with deterministic boundary validation errors.

### AC-2 Valid identifiers retain canonical behavior
Given valid network/subject identifiers,
when identities are generated,
then canonical lowercase normalization and deterministic outputs remain unchanged.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Unit/Conformance | network label starts with `-` (`-edge.tau`) | build identity | error contains boundary validation message for network |
| C-02 | AC-1 | Unit/Conformance | network label ends with `_` (`edge_.tau`) | build identity | error contains boundary validation message for network |
| C-03 | AC-1 | Unit/Conformance | subject label ends with `-` (`agent-primary-`) | build identity | error contains boundary validation message for subject |
| C-04 | AC-2 | Functional/Conformance | valid mixed-case padded identifiers | build identity | normalized lowercase output preserved and request succeeds |

## Success Metrics / Observable Signals
- `cargo test -p kamn-core spec_3168 -- --test-threads=1`
- `cargo test -p kamn-core`
- `cargo fmt --check`
- `cargo clippy -p kamn-core -- -D warnings`
