# Training Crate Boundary Plan

- Generated: 2026-02-15T00:00:00Z
- Scope: tau-training-* crates + tau-trainer + tau-algorithm

## Summary

| Metric | Value |
| --- | ---: |
| Total crates | 7 |
| Retain decisions | 7 |
| Merge decisions | 0 |
| Ambiguous decisions | 0 |

## Decision Matrix

| Crate | Decision | Merge Target | Owner Surface | Rationale |
| --- | --- | --- | --- | --- |
| `tau-algorithm` | retain | - | strategy layer (APO + adapters) | Algorithm surface evolves separately from runtime/store plumbing and keeps strategy polymorphism clean. |
| `tau-trainer` | retain | - | top-level fit orchestration and lifecycle coordination | Keeps orchestration boundary explicit above runner/store without forcing algorithm coupling. |
| `tau-training-proxy` | retain | - | optional OpenAI-compatible attribution proxy | Operationally optional HTTP surface; should remain isolated from core prompt optimization runtime. |
| `tau-training-runner` | retain | - | worker poll-execute-report loop | Runner behavior remains independently testable and can scale without coupling to trainer orchestration. |
| `tau-training-store` | retain | - | rollout queue, persistence, and resource versioning | SQLite and in-memory store boundaries are stable and used by multiple runtime surfaces. |
| `tau-training-tracer` | retain | - | execution spans and reward emission contracts | Tracer integrates with agent events and store without owning runner orchestration. |
| `tau-training-types` | retain | - | shared training domain types and serde contracts | Leaf types crate used by store/runner/tracer/algorithm/trainer; avoids cyclic dependencies. |

## First Consolidation PR Sets

| Set | Status | Issues | Scope | Test Matrix |
| --- | --- | --- | --- | --- |
| `training-boundary-set-a` | completed | #1711 | Publish crate-by-crate retain/merge decisions.; Wire decision-plan checks and docs references. | unit, functional, integration, regression |
| `training-boundary-set-b` | planned | #1712 | Remove stale training alias/docs paths after boundary confirmation.; Align CLI/help output with prompt-optimization naming. | unit, functional, integration, regression |
| `training-boundary-set-c` | planned | #1628 | Implement merges only where future ambiguity appears or duplication emerges.; Preserve compile/test stability across trainer/runner/store/algorithm surfaces. | unit, functional, integration, regression |
