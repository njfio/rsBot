# Panic/Unsafe Policy

This policy tracks and ratchets `panic!` and Rust `unsafe` keyword usage.

## Goals
- Keep production-facing `panic!` and `unsafe` usage audited and justified.
- Detect unreviewed growth deterministically.
- Preserve explicit baseline metadata for auditable re-baselining.

## Commands
Audit current repository state:

```bash
scripts/dev/panic-unsafe-audit.sh
```

Enforce baseline thresholds:

```bash
scripts/dev/panic-unsafe-guard.sh
```

## Baseline Artifact
Baseline metadata lives at:

- `tasks/policies/panic-unsafe-baseline.json`

Required fields:
- `schema_version`
- `policy_id`
- `owner_issue`
- `approved_by`
- `approved_at`
- `rationale`
- `thresholds`

Thresholds currently enforced:
- `panic_total_max`
- `panic_review_required_max`
- `unsafe_total_max`
- `unsafe_review_required_max`

## Classification Buckets
The audit script classifies occurrences into buckets:
- `path_test`: paths under test/bench/example conventions.
- `cfg_test_module`: within `#[cfg(test)]` module scope.
- `inline_test`: near `#[test]`/`#[tokio::test]`/`#[rstest]` annotations.
- `review_required`: not heuristically identified as test-only; requires review/remediation.

## Re-baseline Process
1. Run `scripts/dev/panic-unsafe-audit.sh` and review findings.
2. If growth is intentional and justified, update `tasks/policies/panic-unsafe-baseline.json`:
   - bump thresholds explicitly,
   - update `owner_issue`, `approved_by`, `approved_at`, and `rationale`.
3. Attach rationale and review evidence in the linked issue/PR.
