# Issue 1656 Plan

Status: Reviewed

## Approach

1. Add spot-audit checklist/report artifacts under `tasks/reports/`.
2. Use `m23-doc-quality-audit-helper` output as quantitative signal.
3. Calibrate overly broad heuristic pattern in
   `tasks/policies/doc-quality-anti-patterns.json` if false-positive rate is high.
4. Re-run helper and publish before/after evidence in audit report.
5. Update remediation guide with spot-audit report references.

## Affected Areas

- `tasks/policies/doc-quality-anti-patterns.json`
- `tasks/reports/m23-doc-quality-audit-helper.json`
- `tasks/reports/m23-doc-quality-audit-helper.md`
- `tasks/reports/m23-doc-quality-spot-audit.json`
- `tasks/reports/m23-doc-quality-spot-audit.md`
- `docs/guides/doc-quality-remediation.md`
- `specs/1656/*`

## Risks And Mitigations

- Risk: heuristic calibration hides meaningful low-value findings
  - Mitigation: keep TODO/TBD/FIXME and explicit narration checks intact; only
    narrow noisy broad-verb pattern.
- Risk: audit score lacks objective basis
  - Mitigation: explicit scoring dimensions and threshold in JSON + Markdown.

## ADR

No architecture/dependency/protocol changes. ADR not required.
