# Tasks #2465 - add heartbeat scheduler policy reload path + conformance tests

1. T1 (RED): add failing conformance tests C-01..C-03 in `tau-runtime` heartbeat tests.
2. T2 (GREEN): implement sidecar policy reload state machine and interval apply path.
3. T3 (GREEN): add deterministic reason-code/diagnostic emission for applied + invalid reload events.
4. T4 (REGRESSION): run scoped regression tests for existing heartbeat scheduler behaviors.
5. T5 (VERIFY): run `cargo fmt --check`, `cargo clippy -p tau-runtime -- -D warnings`, and scoped `cargo test -p tau-runtime`.

## Test Tier Mapping
- Unit: policy parse/validation helpers.
- Functional: heartbeat cycle behavior under updated interval.
- Conformance: C-01..C-03 named tests.
- Integration: scheduler end-to-end apply path.
- Regression: no-change and invalid-policy stability.
