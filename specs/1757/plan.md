# Issue 1757 Plan

Status: Reviewed

## Approach

1. Add `scripts/dev/doc-density-gate-artifact.sh` to orchestrate
   `.github/scripts/rust_doc_density.py` and emit standardized JSON/Markdown
   gate artifacts.
2. Add a script contract test (`scripts/dev/test-doc-density-gate-artifact.sh`)
   covering functional and regression behavior.
3. Add a docs contract test under `.github/scripts/` to enforce discoverability
   of script usage, artifact template, and troubleshooting guidance.
4. Update `docs/guides/doc-density-scorecard.md` with:
   - reproducible gate artifact invocation
   - artifact template fields
   - troubleshooting runbook notes

## Affected Areas

- `scripts/dev/doc-density-gate-artifact.sh` (new)
- `scripts/dev/test-doc-density-gate-artifact.sh` (new)
- `.github/scripts/test_doc_density_gate_artifact_contract.py` (new)
- `docs/guides/doc-density-scorecard.md` (updated)
- `docs/README.md` (updated index entry)

## Output Contracts

JSON artifact minimum fields:

- `schema_version`
- `generated_at`
- `repo_root`
- `command` (`script`, `targets_file`, `rendered`)
- `versions`
- `context`
- `density_report` (embedded payload from rust-doc-density run)
- `troubleshooting`

Markdown artifact minimum sections:

- header with generation metadata
- command + version/context tables
- summary and per-crate rows
- troubleshooting checklist
- reproduction command block

## Risks And Mitigations

- Risk: environment variance (different tool versions) causes confusion
  - Mitigation: include explicit version capture in artifact.
- Risk: count drift interpreted as script regression
  - Mitigation: include targets file path and rendered command in artifacts.
- Risk: docs drift from script contract
  - Mitigation: add contract test validating scorecard section anchors.

## ADR

No dependency or architectural protocol change. ADR not required.
