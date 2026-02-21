# Plan: Issue #3160 - Sync Review #35 property-gap closure wording and conformance guard

## Approach
1. Add RED script expectations for `Property-based testing` = `**Done**` and updated remaining summary.
2. Update `tasks/review-35.md` closure row and summary wording.
3. Re-run conformance script and formatting checks.

## Affected Modules
- `tasks/review-35.md`
- `scripts/dev/test-review-35.sh`

## Risks & Mitigations
- Risk: brittle conformance assertions from wording drift.
  - Mitigation: keep assertions on deterministic row/summary fragments only.

## Interfaces / Contracts
- Review #35 closure table row value for `Property-based testing`.
- Review #35 remaining-summary sentence.

## ADR
No ADR required.
