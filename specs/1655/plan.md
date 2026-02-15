# Issue 1655 Plan

Status: Reviewed

## Approach

1. Define ratchet policy JSON with floor and baseline artifact paths.
2. Add `scripts/dev/rustdoc-marker-ratchet-check.sh` to:
   - load policy
   - compare current total against floor
   - emit JSON/Markdown report with per-crate deltas
   - return non-zero on floor regression
3. Add script contract test for pass/fail paths.
4. Wire into `.github/workflows/ci.yml` with artifact upload.

## Affected Areas

- `tasks/policies/m23-doc-ratchet-policy.json`
- `scripts/dev/rustdoc-marker-ratchet-check.sh`
- `scripts/dev/test-rustdoc-marker-ratchet-check.sh`
- `.github/workflows/ci.yml`
- `docs/guides/doc-density-scorecard.md`

## Risks And Mitigations

- Risk: floor set too high and blocks all PRs.
  - Mitigation: initialize floor to current-master marker total.
- Risk: artifact schema drift.
  - Mitigation: add fixture-based script contract tests.

## ADR

No architecture/dependency/protocol change. ADR not required.
