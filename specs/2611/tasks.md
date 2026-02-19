# Tasks: Issue #2611 - Provider outbound token-bucket limiter

## Ordered Tasks
1. T1 (RED): add CLI parsing tests for new provider rate-limit flags.
2. T2 (RED): add provider client tests for limiter delay and fail-closed wait-budget behavior.
3. T3 (GREEN): implement token-bucket limiter + async wrapper in `tau-provider`.
4. T4 (GREEN): wire wrapper through `build_provider_client` for provider HTTP clients.
5. T5 (VERIFY): run `cargo fmt --check`, `cargo clippy -p tau-provider -- -D warnings`, `cargo test -p tau-provider`, and `cargo test -p tau-coding-agent cli_provider_rate_limit_flags`.
6. T6 (VERIFY): run `cargo mutants --in-diff`.
7. T7 (CLOSE): update issue process log with AC/test mapping evidence.

## Tier Mapping
- Unit: C-01
- Property: N/A (no randomized property harness introduced in this slice)
- Contract/DbC: N/A (no `contracts` annotations introduced)
- Snapshot: N/A (no snapshot fixtures)
- Functional: C-02
- Conformance: C-01..C-04
- Integration: C-04
- Fuzz: N/A (no new parser/untrusted surface)
- Mutation: C-05 (`cargo mutants --in-diff`)
- Regression: C-03
- Performance: N/A (no benchmark gate in this slice)
