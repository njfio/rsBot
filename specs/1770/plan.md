# Issue 1770 Plan

Status: Reviewed

## Approach

1. Add cadence policy JSON with expected frequency, grace window, and escalation
   thresholds/actions.
2. Add checklist markdown for tracker operators to complete before posting
   updates.
3. Add cadence-check script that evaluates latest critical-path update timestamp
   from fixture or live issue comments.
4. Add contract tests for policy/checklist/script/docs behavior and stale/missing
   update failure modes.
5. Update roadmap docs index and sync guide with cadence-check workflow.

## Affected Areas

- `tasks/policies/critical-path-update-cadence-policy.json` (new)
- `tasks/templates/critical-path-cadence-checklist.md` (new)
- `scripts/dev/critical-path-cadence-check.sh` (new)
- `.github/scripts/test_critical_path_cadence_policy_contract.py` (new)
- `docs/guides/roadmap-status-sync.md`
- `docs/README.md`

## Output Contracts

Cadence policy JSON includes:

- `schema_version`, `policy_id`
- `tracker_issue_number`
- cadence + grace window hours
- escalation thresholds and escalation path actions
- required checklist item identifiers

Cadence-check script output (JSON mode):

- `status` (`ok|warning|critical`)
- `reason_code`
- `last_update_at`
- `age_hours`
- `cadence_hours`, `grace_period_hours`, escalation thresholds

## Risks And Mitigations

- Risk: ambiguous timestamp parsing for comment timestamps
  - Mitigation: ISO8601 UTC parsing with deterministic fixture tests.
- Risk: false-positive stale alerts
  - Mitigation: explicit grace window and policy-configurable thresholds.

## ADR

No new dependency or protocol changes. ADR not required.
