# M178 - Preflight Fast Safety Guard Integration

## Context
`scripts/dev/preflight-fast.sh` currently enforces roadmap freshness before package-scoped validation, but it does not include panic/unsafe policy guardrails in the same fast-loop command path. Operators requested higher velocity without sacrificing safety; this milestone closes that gap.

## Scope
- Add fail-closed panic/unsafe guard execution to `preflight-fast.sh` before `fast-validate`.
- Preserve argument passthrough behavior to `fast-validate`.
- Extend deterministic script tests for ordering and fail-closed semantics.

## Linked Issues
- Epic: #2999
- Story: #2998
- Task: #3000
