# Tasks: Issue #3052 - Diagnostics telemetry aggregation hardening

## Ordered Tasks
1. [x] T1 (RED): add failing conformance test for missing `total_tokens` fallback behavior.
2. [x] T2 (RED/GREEN): add conformance test for mixed explicit/fallback total-token aggregation.
3. [x] T3 (GREEN): implement minimal fallback aggregation runtime change.
4. [x] T4 (REGRESSION): rerun targeted and full `tau-diagnostics` tests.
5. [x] T5 (VERIFY): run `cargo fmt --check`, `cargo clippy -p tau-diagnostics -- -D warnings`, and `cargo check -q`.

## Tier Mapping
- Unit: aggregation behavior tests in diagnostics test module
- Property: N/A (no randomized invariant requirement in this slice)
- Contract/DbC: N/A (no contracts crate annotations)
- Snapshot: N/A (no snapshot output surface)
- Functional: audit summarization behavior checks
- Conformance: C-01..C-03
- Integration: mixed-record JSONL fixture processing in real summarizer path
- Fuzz: N/A (no new untrusted parser surface)
- Mutation: N/A (non-critical targeted hardening slice)
- Regression: targeted + full crate reruns with lint/check gates
- Performance: N/A (no hotspot/perf-path changes)
