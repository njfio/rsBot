# Training Crate Boundary Plan

This guide defines the concrete merge/retain boundary decisions for Tau's training crate stack.

Scope:
- `tau-training-types`
- `tau-training-store`
- `tau-training-tracer`
- `tau-training-runner`
- `tau-training-proxy`
- `tau-trainer`
- `tau-algorithm`

## Decision summary

Current decision baseline: retain all seven crates with explicit ownership boundaries.

Rationale for retain-first strategy:
- the stack is already layered and acyclic
- each crate maps to a distinct runtime responsibility
- premature merges would increase coupling without reducing operational complexity

## Machine-readable plan artifacts

Generate plan artifacts:

```bash
./scripts/dev/training-crate-boundary-plan.sh
```

Default outputs:
- `tasks/reports/training-crate-boundary-plan.json`
- `tasks/reports/training-crate-boundary-plan.md`

Schema:
- `tasks/schemas/training-crate-boundary-plan.schema.json`

Validation tests:

```bash
./scripts/dev/test-training-crate-boundary-plan.sh
```

## First consolidation PR sets

The plan publishes explicit PR sets with issue linkage:

1. `training-boundary-set-a` (`#1711`)  
   Boundary decisions + docs/check contracts.
2. `training-boundary-set-b` (`#1712`)  
   Remove stale training flag/docs references after boundary confirmation.
3. `training-boundary-set-c` (`#1628`)  
   Follow-through consolidation changes only when ambiguity/duplication appears.

## Ownership

Primary ownership surfaces:
- `scripts/dev/training-crate-boundary-plan.sh` (canonical decision-plan artifact generator)
- `crates/tau-trainer` (top-level orchestration)
- `crates/tau-training-store` + `crates/tau-training-types` (state/model boundaries)
- `crates/tau-training-runner` + `crates/tau-training-tracer` (execution and telemetry)
- `crates/tau-algorithm` (strategy layer)
- `crates/tau-training-proxy` (optional attribution proxy surface)

Ownership map: `docs/guides/runbook-ownership-map.md`.
