# Issue 1703 Tasks

Status: Implementing

## Ordered Tasks

T1 (tests-first): extend matrix script contract tests to assert repo-relative
artifact paths (no absolute prefixes).

T2: update matrix script artifact path serialization to relative paths.

T3: regenerate live M21 matrix reports and validate outputs.

T4: post gate evidence summary on #1703 and close issue.

## Tier Mapping

- Unit: script syntax and deterministic fixture checks
- Functional: matrix generation output existence/summary checks
- Regression: relative-path contract checks
- Integration: gate issue publication with artifact references
- Conformance: C-01..C-03 coverage through script tests + issue evidence
