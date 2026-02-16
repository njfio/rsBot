# Issue 1679 Plan

Status: Reviewed

## Approach

1. Tests-first:
   - add `scripts/dev/test-cli-args-domain-split.sh` to check:
     - `cli_args.rs` line count `< 4000`
     - flattened module marker for extracted tail-domain flags
     - extracted domain file exists with expected marker flags
   - run harness for RED against current line count / missing extraction
2. Implement split:
   - create `crates/tau-cli/src/cli_args/runtime_tail_flags.rs` containing
     custom-command/voice/github field definitions
   - wire `cli_args.rs` to `CliRuntimeTailFlags` via clap `#[command(flatten)]`
3. Run harness for GREEN.
4. Run targeted CLI behavior tests and quality checks:
   - `cargo test -p tau-coding-agent cli_validation`
   - `scripts/dev/roadmap-status-sync.sh --check --quiet`
   - `cargo fmt --check`
   - `cargo clippy -p tau-cli -- -D warnings`

## Affected Areas

- `crates/tau-cli/src/cli_args.rs`
- `crates/tau-cli/src/cli_args/runtime_tail_flags.rs`
- `scripts/dev/test-cli-args-domain-split.sh`
- `specs/1679/spec.md`
- `specs/1679/plan.md`
- `specs/1679/tasks.md`

## Risks And Mitigations

- Risk: accidental clap metadata regression during extraction.
  - Mitigation: token-for-token move of extracted fields; targeted CLI tests.
- Risk: flatten/module extraction harms readability.
  - Mitigation: domain file path and markers are explicit, scoped to tail
    runtime domains.

## Interfaces / Contracts

- `Cli` public struct field names and types remain unchanged.
- clap arg metadata for extracted fields remains unchanged.
- extraction contract is asserted by the harness script.

## ADR

No dependency/protocol/architecture decision changes; ADR not required.
