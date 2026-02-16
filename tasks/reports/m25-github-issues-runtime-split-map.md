# GitHub Issues Runtime Split Map (M25)

- Generated at (UTC): `2026-02-16T00:00:00Z`
- Source file: `crates/tau-github-issues-runtime/src/github_issues_runtime.rs`
- Target line budget: `3000`
- Current line count: `3390`
- Current gap to target: `390`
- Estimated lines to extract: `450`
- Estimated post-split line count: `2940`

## Extraction Phases

| Phase | Owner | Est. Reduction | Depends On | Modules | Notes |
| --- | --- | ---: | --- | --- | --- |
| phase-1-webhook-validation-ingest (Webhook payload validation and ingest normalization) | github-issues-ingest | 170 | - | github_issues_runtime/webhook_validation.rs, github_issues_runtime/ingest.rs | Preserve webhook signature/shape validation and deterministic ingest diagnostics. |
| phase-2-session-sync-routing (Session projection, comment sync, and routing helpers) | github-issues-sync | 150 | phase-1-webhook-validation-ingest | github_issues_runtime/session_projection.rs, github_issues_runtime/comment_sync.rs, github_issues_runtime/routing.rs | Keep issue-to-session mapping and message emission order stable. |
| phase-3-error-policy-rate-limit (Policy guards, rate-limits, and error-envelope mapping) | github-issues-policy | 130 | phase-2-session-sync-routing | github_issues_runtime/policy.rs, github_issues_runtime/rate_limit.rs, github_issues_runtime/errors.rs | Retain fail-closed behavior and reason-code mapping for moderation and retry signals. |

## Public API Impact

- Keep GitHub Issues runtime public entrypoints and bridge configuration surfaces stable.
- Preserve webhook ingest and issue-comment processing payload contracts.
- Maintain existing reason-code/error-envelope behavior exposed to callers.

## Import Impact

- Introduce module declarations under crates/tau-github-issues-runtime/src/github_issues_runtime/ with selective re-exports.
- Move ingest/sync/policy helper domains out of github_issues_runtime.rs in phases.
- Keep shared bridge utility helpers centralized to minimize cross-module import churn.

## Test Migration Plan

| Order | Step | Command | Expected Signal |
| ---: | --- | --- | --- |
| 1 | guardrail-threshold-enforcement: Introduce and enforce github_issues_runtime.rs split guardrail ending at <3000. | scripts/dev/test-github-issues-runtime-domain-split.sh | github_issues_runtime.rs threshold checks fail closed until split target is reached |
| 2 | runtime-crate-coverage: Run crate-scoped GitHub Issues runtime tests after each extraction phase. | cargo test -p tau-github-issues-runtime | bridge ingest/sync behavior and reason-code tests stay green |
| 3 | runtime-integration: Run cross-crate integration suites that consume GitHub Issues runtime surfaces. | cargo test -p tau-coding-agent | no regressions in issue bridge wiring and end-to-end command flows |
