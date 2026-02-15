# Issue 1734 Plan

Status: Reviewed

## Approach

1. Add anti-pattern policy:
   - `tasks/policies/doc-quality-anti-patterns.json`
   - regex/substring heuristics + suppression entries
2. Add helper command:
   - `scripts/dev/doc-quality-audit-helper.sh`
   - scans `crates/*/src/**/*.rs` rustdoc lines
   - outputs JSON + Markdown findings
3. Add contract test:
   - `scripts/dev/test-doc-quality-audit-helper.sh`
   - fixture-based finding and suppression checks
4. Update remediation docs with false-positive handling instructions.

## Affected Areas

- `tasks/policies/doc-quality-anti-patterns.json` (new)
- `scripts/dev/doc-quality-audit-helper.sh` (new)
- `scripts/dev/test-doc-quality-audit-helper.sh` (new)
- `docs/guides/doc-quality-remediation.md` (update)
- `tasks/reports/m23-doc-quality-audit-helper.json` (generated)
- `tasks/reports/m23-doc-quality-audit-helper.md` (generated)

## Risks And Mitigations

- Risk: noisy heuristics trigger excessive false positives
  - Mitigation: suppression entries + summary suppression counts.
- Risk: policy drift from remediation docs
  - Mitigation: docs update in same PR and contract tests.

## ADR

No architecture/dependency/protocol changes. ADR not required.
