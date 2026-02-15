# Issue 1727 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add/confirm failing tests for lifecycle token parsing, policy
authorization allow/deny behavior, enforcement denial diagnostics, and stable
action key mapping.

T2: implement `RlLifecycleAction` parser and `control:rl:<action>` key mapper.

T3: implement authorization and enforcement wrappers over RBAC policy
evaluation (default and explicit policy paths).

T4: run scoped verification (`fmt`, `clippy -p tau-access`, `test -p tau-access`)
and map AC-1..AC-3 to C-01..C-04 evidence.

## Tier Mapping

- Unit: lifecycle token parsing and invalid token rejection
- Functional: authorization decisions with policy fixtures
- Integration: action-key mapping and policy-path based authorization
- Regression: denied lifecycle action enforcement diagnostics
- Conformance: C-01..C-04
