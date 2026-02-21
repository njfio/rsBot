# Spec: Issue #3000 - Integrate panic/unsafe guard into preflight-fast command path

Status: Implemented

## Problem Statement
`scripts/dev/preflight-fast.sh` is the primary local velocity gate, but it currently skips panic/unsafe policy guard checks. This allows local preflight success even when safety ratchets fail, pushing avoidable failures to later CI stages.

## Acceptance Criteria

### AC-1 Preflight-fast runs panic/unsafe guard before fast-validate
Given `scripts/dev/preflight-fast.sh` execution,
When roadmap freshness passes,
Then panic/unsafe guard runs next and only then `fast-validate` executes.

### AC-2 Fail-closed behavior is preserved
Given any non-zero exit from roadmap or panic/unsafe guard checks,
When preflight-fast runs,
Then it exits non-zero and does not invoke `fast-validate`.

### AC-3 Fast-validate argument passthrough remains unchanged
Given args passed to `preflight-fast.sh`,
When guard stages pass,
Then those args are forwarded unchanged to `fast-validate`.

### AC-4 Script-level regression coverage validates sequencing
Given script test suite,
When running `scripts/dev/test-preflight-fast.sh`,
Then tests assert ordered execution and fail-closed behavior for roadmap and guard failures.

## Scope

### In Scope
- `scripts/dev/preflight-fast.sh` sequencing update.
- `scripts/dev/test-preflight-fast.sh` regression coverage expansion.
- milestone/task spec artifacts for M178.

### Out of Scope
- CI workflow YAML changes.
- changes to panic/unsafe guard policy thresholds.
- modifications to fast-validate package scope behavior.

## Conformance Cases
- C-01: preflight success path runs roadmap check -> panic/unsafe guard -> fast-validate.
- C-02: roadmap failure path exits non-zero and skips guard/fast-validate.
- C-03: panic/unsafe guard failure path exits non-zero and skips fast-validate.
- C-04: passthrough args are preserved to fast-validate on success.
- C-05: `scripts/dev/test-preflight-fast.sh` passes.

## Success Metrics / Observable Signals
- `scripts/dev/test-preflight-fast.sh` passes.
- `scripts/dev/test-panic-unsafe-guard.sh` passes.
- `scripts/dev/test-fast-validate.sh` passes.
- `cargo fmt --check` passes.

## Approval Gate
P1 scope: spec authored/reviewed by agent; implementation proceeds and is flagged for human review in PR.
