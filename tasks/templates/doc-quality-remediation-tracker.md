# Doc Quality Remediation Tracker

Use this template for each documentation audit finding tracked under M23.

Reference policy: `tasks/policies/doc-quality-remediation-policy.json`

## Finding Metadata

| Field | Value |
| --- | --- |
| finding_id | `<finding-id>` |
| severity | `critical|high|medium|low` |
| owner | `<github-handle-or-team>` |
| detected_at | `<YYYY-MM-DD>` |
| due_at | `<YYYY-MM-DD>` |
| source_artifact | `<report/log/link>` |
| linked_issue | `<issue-url>` |

## Severity And SLA

| Severity | Target SLA (hours) | Use when |
| --- | ---: | --- |
| critical | 24 | Safety/compliance risk or gate-blocking docs gap |
| high | 72 | Materially incomplete/misleading operational docs |
| medium | 168 | Consistency/clarity debt with moderate impact |
| low | 336 | Minor wording/format/discoverability fixes |

## Remediation Checklist

- [ ] Confirm finding scope and impacted docs/commands.
- [ ] Assign owner and due date based on severity SLA.
- [ ] Document remediation steps and linked PR/issues.
- [ ] Capture validation evidence (tests/check outputs).
- [ ] Record reviewer sign-off before closure.

## Closure Proof

- [ ] finding_id
- [ ] severity
- [ ] owner
- [ ] source_artifact
- [ ] root_cause
- [ ] remediation_summary
- [ ] validation_evidence
- [ ] reviewer
- [ ] closed_at

### Closure Proof Entry

| Field | Value |
| --- | --- |
| finding_id | `<finding-id>` |
| severity | `<critical|high|medium|low>` |
| owner | `<github-handle-or-team>` |
| source_artifact | `<report/log/link>` |
| root_cause | `<what failed and why>` |
| remediation_summary | `<changes shipped>` |
| validation_evidence | `<tests/check commands + outputs>` |
| reviewer | `<review-approver>` |
| closed_at | `<YYYY-MM-DDTHH:MM:SSZ>` |
