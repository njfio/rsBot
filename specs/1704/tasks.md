# Issue 1704 Tasks

Status: Implementing

## Ordered Tasks

T1 (tests-first): extend retained proof summary test contract to require
repository-relative emitted paths.

T2: update retained proof summary script path serialization.

T3: execute live retained-capability proof summary command and regenerate report
artifacts.

T4: publish proof evidence + troubleshooting notes in #1704 and close issue.

## Tier Mapping

- Unit: script syntax and fixture contract execution
- Functional: proof summary generation and status fields
- Regression: relative-path portability checks
- Integration: issue publication with artifact/troubleshooting evidence
- Conformance: C-01..C-03 coverage via script tests + gate comment
