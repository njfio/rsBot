# Issue 1691 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): capture onboarding module header gap list as RED evidence.

T2: add startup phase `//!` docs across preflight/resolution/dispatch modules.

T3: add onboarding wizard/profile/transport mode boundary docs with invariants
and failure semantics.

T4: run targeted onboarding crate tests and docs checks.

## Tier Mapping

- Functional: module boundary docs present across onboarding files
- Conformance: startup/wizard/transport contracts documented in headers
- Regression: onboarding crate tests + docs link checks remain green
