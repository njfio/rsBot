# Issue 1758 Plan

Status: Reviewed

## Approach

1. Add a policy artifact in `tasks/policies/` defining severity classes,
   SLA targets, and closure-proof field contracts.
2. Add a reusable remediation tracking template in `tasks/templates/` with:
   - severity + SLA metadata fields
   - remediation checklist
   - closure proof section
3. Add a guide in `docs/guides/` documenting the workflow and linking the
   policy/template artifacts.
4. Add contract tests under `.github/scripts/` validating:
   - policy shape
   - template required sections
   - policy/template/docs alignment

## Affected Areas

- `tasks/policies/doc-quality-remediation-policy.json` (new)
- `tasks/templates/doc-quality-remediation-tracker.md` (new)
- `docs/guides/doc-quality-remediation.md` (new)
- `docs/README.md` (updated index entry)
- `.github/scripts/test_doc_quality_remediation_contract.py` (new)

## Output Contracts

Policy minimum fields:

- `schema_version`
- `policy_id`
- `severity_classes[]` (`id`, `name`, `definition`, `target_sla_hours`)
- `closure_proof_fields[]`
- `checklist_items[]`

Template minimum sections:

- finding metadata table
- severity/SLA mapping
- remediation checklist
- closure proof fields checklist

Guide minimum coverage:

- how to classify severity
- SLA expectations and escalation notes
- how to complete closure proof entries

## Risks And Mitigations

- Risk: policy and template drift over time
  - Mitigation: cross-file contract tests check alignment.
- Risk: unclear ownership of remediation entries
  - Mitigation: template requires owner + due date fields.

## ADR

No new dependencies or protocol changes. ADR not required.
