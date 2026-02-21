# Plan: Issue #2992 - cli_args runtime feature tail extraction

## Approach
1. Capture RED baseline: root file line count and focused CLI validation test.
2. Move the post-`execution_domain` tail field declarations into dedicated include artifact(s) under `crates/tau-cli/src/cli_args/`.
3. Wire include(s) into `Cli` struct without changing flag attributes.
4. Run scoped and crate-level regressions, then fmt/clippy gates.
5. Validate line-count delta against baseline.

## Affected Modules
- `crates/tau-cli/src/cli_args.rs`
- `crates/tau-cli/src/cli_args/*.rs` (new include artifact(s))
- `specs/milestones/m176/index.md`
- `specs/2992/{spec.md,plan.md,tasks.md}`

## Risks and Mitigations
- Risk: accidental clap contract drift during move.
  - Mitigation: copy field blocks without semantic edits; run CLI validation tests.
- Risk: include placement or syntax breaks derive parsing.
  - Mitigation: compile/test quickly after extraction and keep changes minimal.

## Interfaces / Contracts
- `pub struct Cli` remains the external API surface.
- No flag name/env/default changes for extracted fields.
