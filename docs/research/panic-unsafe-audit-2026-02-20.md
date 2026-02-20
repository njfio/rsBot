# Panic/Unsafe Audit Report (2026-02-20)

## Scope
Issue: #2927

Audit command:

```bash
scripts/dev/panic-unsafe-audit.sh --output-json /tmp/panic-unsafe-audit-2927.json
```

## Results
- `panic_total`: 122
- `panic_review_required`: 0
- `panic_cfg_test_module`: 47
- `panic_path_test`: 75
- `unsafe_total`: 3
- `unsafe_review_required`: 0
- `unsafe_path_test`: 3

## Key Findings
1. No `panic!` occurrences were classified into `review_required`.
2. All `panic!` occurrences were classified as test-only (`cfg_test_module` or `path_test`).
3. All Rust `unsafe` keyword occurrences were classified as test-only, all in `crates/tau-session/src/tests.rs`.

Top `panic!` concentration (by count):
- `crates/tau-onboarding/src/startup_local_runtime/tests.rs` (17)
- `crates/tau-onboarding/src/startup_dispatch.rs` test module (12)
- `crates/tau-agent-core/src/tests/safety_pipeline.rs` (9)

## Controls Added
- Audit tool: `scripts/dev/panic-unsafe-audit.sh`
- Guardrail tool: `scripts/dev/panic-unsafe-guard.sh`
- Baseline policy: `tasks/policies/panic-unsafe-baseline.json`
- Policy guide: `docs/guides/panic-unsafe-policy.md`
- Script tests:
  - `scripts/dev/test-panic-unsafe-audit.sh`
  - `scripts/dev/test-panic-unsafe-guard.sh`

## Operational Guidance
- Run guardrail before PRs touching tests/runtime safety handling:

```bash
scripts/dev/panic-unsafe-guard.sh
```

- If intentional growth is required, update baseline metadata with issue-linked rationale and reviewer approval.
