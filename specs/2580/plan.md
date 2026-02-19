# Plan #2580

## Approach
1. Run mapped #2579 conformance/regression tests and capture pass evidence.
2. Run scoped `cargo fmt --check`, `cargo clippy -p tau-agent-core -- -D warnings`, and `cargo test -p tau-agent-core`.
3. Run `cargo mutants --in-diff` on #2579 diff and fix any missed mutants.
4. Run sanitized provider live smoke and capture summary.
5. Update issue logs/checklists and package evidence in PR.

## Risks & Mitigations
- Risk: mutation escapes identify missing assertions in warn-tier LLM path.
  - Mitigation: add targeted regression tests before finalization.
- Risk: provider key drift causes live-smoke failures.
  - Mitigation: run sanitized keyfile with deterministic skip strategy.

## Interfaces / Contracts
- Verification-only subtask; no external behavior changes expected.
