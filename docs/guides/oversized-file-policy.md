# Oversized File Policy

This policy defines line-count thresholds for production source files and the
temporary exemption process used when staged refactors are in flight.

## Default Thresholds

- Production Rust source files (`crates/*/src/**/*.rs`): `4000` lines
- Absolute temporary exemption cap: `8000` lines
- Exemption lifespan: time-bounded and explicitly reviewed before expiry

Guidance:

- Keep modules below the default threshold by splitting into focused domain
  modules before feature growth continues.
- Use temporary exemptions only when an active split plan is already scheduled.

## Exemption Metadata Contract

Exemptions are tracked in `tasks/policies/oversized-file-exemptions.json`.

Each exemption entry must include:

- `path`: file path under version control
- `threshold_lines`: temporary line budget for that file
- `owner_issue`: active GitHub issue id tracking the split/remediation
- `rationale`: explicit reason the exemption is needed now
- `approved_by`: reviewer/maintainer that approved the exemption
- `approved_at`: approval date (`YYYY-MM-DD`)
- `expires_on`: expiry date (`YYYY-MM-DD`)

The validator script enforces:

- schema version
- required fields
- threshold bounds (`> default`, `<= max`)
- non-expired exemptions
- no duplicate file-path exemptions

## Review and Expiry Process

1. Open or link a remediation issue before adding an exemption.
2. Add exemption metadata with a bounded expiry window.
3. Run policy validation:

```bash
scripts/dev/oversized-file-policy.sh
```

4. Before `expires_on`, either:
   - remove exemption because file is now under threshold, or
   - renew with updated rationale, approval, and a new bounded expiry date.
5. Expired exemptions fail validation and must be resolved immediately.

## CI Output Linkage

Policy-validation output must include this guide path:

- `docs/guides/oversized-file-policy.md`

This keeps failure messages actionable and makes exemption decisions auditable.
