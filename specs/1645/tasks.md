# Issue 1645 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add safety live-run validation harness and capture RED on
missing demo-index safety-smoke docs section.

T2: update demo-index guide with explicit safety-smoke scenario and CI-light
coverage notes.

T3: run GREEN harness and targeted demo safety smoke test.

T4: run roadmap/fmt/clippy checks and prepare PR closure evidence.

## Tier Mapping

- Functional: safety live-run contract harness
- Integration: `scripts/demo/test-safety-smoke.sh`
- CI verification: harness checks for manifest/workflow smoke wiring
