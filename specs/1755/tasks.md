# Issue 1755 Tasks

Status: Implementing

## Ordered Tasks

T1 (tests-first): add RED scanner contract test covering approved/stale
classification and regression error modes.

T2: implement allowlist policy schema with examples/non-examples.

T3: implement terminology scanner script with JSON/Markdown outputs.

T4: add docs guide and docs index linkage; run targeted tests to GREEN.

## Tier Mapping

- Functional: policy/schema presence + scanner run emits outputs
- Conformance: approved/stale classification aligns with allowlist constraints
- Regression: invalid args/missing policy/incorrect context remain detectable
