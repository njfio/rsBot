# Tasks: Issue #2607 - Revalidate tau-gaps roadmap and implement open P0/P1 hygiene-safety slice

## Ordered Tasks
1. T1 (RED): add `tau-safety` tests for obfuscated prompt-injection/leak variants and redaction edge cases.
2. T2 (GREEN): implement scanner/leak-detector hardening only for RED failures.
3. T3 (VERIFY): run `cargo test -p tau-safety` and scoped `cargo clippy -p tau-safety -- -D warnings`.
4. T4 (GREEN): add missing artifacts `.env.example`, `CHANGELOG.md`, and `rustfmt.toml`.
5. T5 (GREEN): create follow-up issues for remaining non-trivial open roadmap items.
6. T6 (VERIFY): update `tasks/tau-gaps-issues-improvements.md` with per-item validated status + evidence + follow-up links.
7. T7 (VERIFY): run `cargo fmt --check` and final targeted tests.

## Tier Mapping
- Unit: scanner/leak-detector edge-case tests in `tau-safety`
- Functional: roadmap item status/evidence refresh for all 23 items
- Conformance: C-01..C-04
- Integration: N/A (single-crate behavior + docs/hygiene artifacts in this slice)
- Regression: obfuscated input detection/redaction cases in `tau-safety`
- Property: N/A (no randomized invariant surface added)
- Contract/DbC: N/A (no DbC macro adoption in this slice)
- Snapshot: N/A (no snapshot artifact contract in this slice)
- Fuzz: Existing fuzz harness remains; no new target in this slice
- Mutation: N/A for docs/hygiene and scanner test expansion slice
- Performance: N/A (no hotspot path or benchmarked algorithm change)
