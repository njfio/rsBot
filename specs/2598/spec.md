# Spec #2598 - Subtask: package conformance/live validation evidence for G16 hot-reload rollout

Status: Implemented
Priority: P1
Milestone: M102
Parent: #2597

## Problem Statement
The G16 completion slice in #2597 changes runtime hot-reload behavior and requires reproducible verification evidence (tests, mutation, live smoke, process logs) before merge.

## Scope
- Re-run #2597 conformance suite.
- Run scoped quality gates and mutation-in-diff for touched paths.
- Run sanitized live validation smoke.
- Update closure artifacts (spec status, tasks evidence, issue logs).

## Out of Scope
- New feature work beyond #2597 ACs.

## Acceptance Criteria
- AC-1: #2597 conformance cases are reproducibly green.
- AC-2: mutation-in-diff reports zero missed mutants for #2597 changes.
- AC-3: sanitized live validation reports no failures.
- AC-4: issue/spec/task closure artifacts are complete and traceable.

## Conformance Cases
- C-01 (AC-1): mapped #2597 commands pass.
- C-02 (AC-2): `cargo mutants --in-diff <issue2597-diff> -p tau-coding-agent`.
- C-03 (AC-3): sanitized `scripts/dev/provider-live-smoke.sh` summary reports `failed=0`.
- C-04 (AC-4): issue closure comments + spec/task evidence updates are present.

## Verification Evidence
- C-01: `cargo test -p tau-coding-agent 2597_ -- --test-threads=1` => pass (4 passed, 0 failed).
- C-02: `cargo mutants --in-place --in-diff /tmp/issue2597-working.diff -p tau-coding-agent --baseline skip --timeout 180 -- --test-threads=1 runtime_profile_policy_bridge::tests::` => `42 mutants tested in 4m: 12 caught, 30 unviable`, with `missed=0`, `timeout=0`.
- C-03: `env -u OPENAI_API_KEY -u TAU_API_KEY -u OPENROUTER_API_KEY -u TAU_OPENROUTER_API_KEY -u DEEPSEEK_API_KEY -u TAU_DEEPSEEK_API_KEY -u XAI_API_KEY -u MISTRAL_API_KEY -u GROQ_API_KEY -u ANTHROPIC_API_KEY -u GEMINI_API_KEY -u GOOGLE_API_KEY TAU_PROVIDER_KEYS_FILE=/tmp/provider-keys-empty.env ./scripts/dev/provider-live-smoke.sh` => `provider-live-smoke summary: ok=0 skipped=8 failed=0`.
- C-04: spec/task files updated to `Implemented` with command evidence and ready for issue closure comments.
