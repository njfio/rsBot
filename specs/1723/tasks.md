# Issue 1723 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add annotation script unit tests for mapping and output formatting.

T2: implement annotation script and CLI contract.

T3: integrate script in CI after doc density check.

T4: run script and CI contract tests.

## Tier Mapping

- Functional: failed crates map to changed files
- Conformance: file-level annotation output includes file/line hints
- Integration: workflow executes annotation step with doc density artifact input
- Regression: script tests and docs checks pass
