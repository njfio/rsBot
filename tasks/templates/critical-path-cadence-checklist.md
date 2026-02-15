# Critical-Path Cadence Checklist

Use this checklist before posting each recurring critical-path update to `#1678`.

- [ ] `dependency_drift_check`: run `scripts/dev/dependency-drift-check.sh --mode dry-run`.
- [ ] `critical_path_template_used`: populate `tasks/templates/critical-path-update-template.md`.
- [ ] `risk_scores_and_rationales_present`: include `low|med|high` risk scores with rationale per item.
- [ ] `blockers_have_owners`: each blocker has owner + next action + target date.
- [ ] `next_update_target_set`: include next expected update timestamp in UTC.

Escalation acknowledgment:

- [ ] If update is stale beyond cadence window, apply escalation path from `tasks/policies/critical-path-update-cadence-policy.json`.
