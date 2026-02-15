# Issue 1767 Tasks

Status: Implementing

## Ordered Tasks

T1 (tests-first): add fixture-driven contract test coverage for extractor output
shape and anomaly surfacing.

T2: implement extractor script with:

- live mode (`gh api`) + retry handling
- fixture mode
- normalized JSON and Markdown outputs

T3: wire docs updates for roadmap operators (usage and validation commands).

T4: run targeted and regression test matrix; capture evidence in issue/PR.

## Tier Mapping

- Functional: extractor execution in fixture mode and expected output creation
- Conformance: JSON/Markdown output fields and hierarchy mapping checks
- Regression: malformed fixture and missing-link/orphan behavior checks
- Integration: contract test invokes script as external command and validates
  emitted files
