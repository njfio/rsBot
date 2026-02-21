# Tasks: Issue #2992 - cli_args runtime feature tail extraction

1. [x] T1 (RED): capture baseline line count and run focused CLI validation test proving current contract.
2. [x] T2 (GREEN): extract post-`execution_domain` runtime feature field block into include artifact(s) and wire into `Cli`.
3. [x] T3 (REGRESSION): run `cargo test -p tau-cli` and `cargo test -p tau-coding-agent cli_validation -- --test-threads=1`.
4. [x] T4 (VERIFY): run `cargo fmt --check` and `cargo clippy -p tau-cli -- -D warnings`.
5. [x] T5 (CONFORMANCE): verify root `cli_args.rs` line count reduction >= 400 lines and document delta.

## Tier Mapping
- Unit: existing tau-cli unit/integration tests.
- Property: N/A (structural refactor only).
- Contract/DbC: N/A (no contract macro surface changes).
- Snapshot: N/A (no snapshot contract changes).
- Functional: CLI argument parsing behavior via existing validation tests.
- Conformance: C-01..C-04.
- Integration: tau-coding-agent CLI validation path.
- Fuzz: N/A (no new untrusted parser logic).
- Mutation: N/A (structural refactor only).
- Regression: focused CLI validation rerun.
- Performance: N/A (no runtime behavior/perf contract change).
