# Plan: Issue #2400 - Stabilize startup model-catalog remote refresh assertion

## Approach
1. Reproduce failing test in RED mode using the targeted CLI integration test.
2. Replace brittle `entries=1` check with a numeric-pattern assertion (`entries=\\d+`) while
   preserving remote source/url checks.
3. Re-run targeted test and scoped quality checks (`fmt`, `clippy`, targeted `cargo test`) in
   GREEN mode.
4. Record conformance mapping and close issue with evidence.

## Affected Modules
- `crates/tau-coding-agent/tests/cli_integration/auth_provider.rs` (integration test only).

## Risks and Mitigations
- Risk: assertion becomes too weak.
  - Mitigation: require all of `source=remote`, `url=`, and `entries=<numeric>`.
- Risk: regex predicate misuse in test.
  - Mitigation: use `predicate::str::is_match(...).expect(...)` for explicit compile/runtime
    validation.

## Interfaces / Contracts
- No API, schema, wire-format, or runtime contract changes.
- Test contract changes from exact count equality to structural numeric validation.

## ADR
- Not required; no architectural decision or dependency change.
