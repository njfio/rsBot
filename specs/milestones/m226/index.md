# M226 - Panic/Unsafe Audit Test-Context Classification

Status: In Progress

## Context
`scripts/dev/audit-panic-unsafe.sh` currently classifies panic/unsafe matches as test/non-test using path-only rules. Test-only code inside `src/*` (`#[cfg(test)]` or `#[test]` contexts) is incorrectly counted as non-test, inflating operational risk metrics.

## Scope
- Add fixture coverage for src-level test-context markers.
- Update audit classification logic to detect test-context by source attributes.
- Preserve existing output format and contract script compatibility.

## Linked Issues
- Epic: #3194
- Story: #3195
- Task: #3196

## Success Signals
- `scripts/dev/test-audit-panic-unsafe.sh`
- `scripts/dev/audit-panic-unsafe.sh`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
