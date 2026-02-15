# Issue 1769 Plan

Status: Reviewed

## Approach

1. Add a reusable markdown template under `tasks/templates/` with required
   critical-path update fields and allowed values.
2. Add a risk rubric policy JSON under `tasks/policies/` defining low/med/high
   criteria and rationale guidance.
3. Add contract tests validating template/rubric fields and docs discoverability.
4. Update roadmap operator docs and docs index to reference the new assets.

## Affected Areas

- `tasks/templates/critical-path-update-template.md` (new)
- `tasks/policies/critical-path-risk-rubric.json` (new)
- `.github/scripts/test_critical_path_update_template_contract.py` (new)
- `docs/guides/roadmap-status-sync.md`
- `docs/README.md`

## Output Contracts

Template markdown includes required sections:

- Update metadata
- Critical path item rows with status field
- Blockers and dependencies
- Risk score (`low|med|high`) + rationale
- Next action and target date

Risk rubric JSON includes:

- `schema_version`, `policy_id`
- allowed risk levels (`low`, `med`, `high`)
- definition + rationale requirements per level
- required status values for critical-path item state

## Risks And Mitigations

- Risk: template not adopted consistently
  - Mitigation: docs index + sync guide references and tracker publication
    example.
- Risk: rubric drift introduces inconsistent risk semantics
  - Mitigation: contract tests assert exact level keys and rationale fields.

## ADR

No new dependency or protocol changes. ADR not required.
