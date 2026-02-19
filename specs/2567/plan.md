# Plan #2567

## Approach
1. Run targeted #2566 tests (conformance + regression naming prefix) and capture pass evidence.
2. Run scoped `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test -p tau-agent-core`.
3. Run `cargo mutants --in-diff` for `tau-agent-core` against the phase-3 diff and fix any escaped mutants.
4. Run sanitized `provider-live-smoke` and record summary metrics.
5. Update process logs and checklist entries, then package evidence in PR/issue comments.

## Risks & Mitigations
- Risk: long-running verification delays cycle time.
  - Mitigation: run prefix-scoped tests first, then full crate suite.
- Risk: flaky live smoke behavior.
  - Mitigation: use local-safe sanitized path and capture deterministic summary output.

## Interfaces / Contracts
- Verification-only subtask; no external runtime contract changes.
