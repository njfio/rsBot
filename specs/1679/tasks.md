# Issue 1679 Tasks

Status: In Progress

## Ordered Tasks

T1 (tests-first): add CLI domain-split contract harness and capture RED on line
budget and missing extraction markers.

T2: extract runtime tail flag domains from `cli_args.rs` into
`crates/tau-cli/src/cli_args/runtime_tail_flags.rs` using flattened domain
module wiring.

T3: run GREEN harness and targeted CLI validation tests.

T4: run roadmap/fmt/clippy checks and prepare PR closure evidence.

## Tier Mapping

- Functional: split contract harness (line budget + extraction markers)
- Regression: targeted CLI validation tests
- Integration: issue verification command sequence
