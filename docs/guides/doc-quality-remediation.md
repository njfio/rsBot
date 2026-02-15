# Doc Quality Remediation Workflow

This guide standardizes how documentation quality findings are triaged,
remediated, and closed with reproducible evidence for M23 gate reviews.

## Source Artifacts

- Policy: `tasks/policies/doc-quality-remediation-policy.json`
- Tracker template: `tasks/templates/doc-quality-remediation-tracker.md`

## Workflow

1. Open a tracker entry using `doc-quality-remediation-tracker.md`.
2. Classify the finding severity (`critical`, `high`, `medium`, `low`).
3. Set `owner` and `due_at` from the severity SLA in the policy.
4. Execute remediation checklist items and link PR/issues.
5. Fill closure proof fields before marking done.

## Severity Classification And SLA

Use policy severities from `doc-quality-remediation-policy.json`:

- `critical` (24h): safety/compliance risk or gate blocker
- `high` (72h): material operational docs gaps
- `medium` (168h): consistency/clarity debt
- `low` (336h): minor wording/format improvements

## Closure Proof Fields

Every closure entry must include these fields:

- `finding_id`
- `severity`
- `owner`
- `source_artifact`
- `root_cause`
- `remediation_summary`
- `validation_evidence`
- `reviewer`
- `closed_at`

## Validation

Run the contract test to verify policy/template/docs alignment:

```bash
python3 -m unittest discover -s .github/scripts -p 'test_doc_quality_remediation_contract.py'
```
