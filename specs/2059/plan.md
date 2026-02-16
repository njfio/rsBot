# Plan #2059

Status: Implemented
Spec: specs/2059/spec.md

## Approach

1. Extract a contiguous high-volume tail of clap fields from
   `cli_args.rs` into a new module struct and wire via `#[command(flatten)]`.
2. Keep field names/attributes unchanged to preserve CLI parsing behavior.
3. Update the split guardrail script from `<4000` to `<3000` and add markers
   for the new extraction module.
4. Execute scoped verification commands and capture evidence.

## Affected Modules

- `crates/tau-cli/src/cli_args.rs`
- `crates/tau-cli/src/cli_args/execution_domain_flags.rs`
- `scripts/dev/test-cli-args-domain-split.sh`

## Risks and Mitigations

- Risk: clap flatten extraction could unintentionally change option wiring.
  - Mitigation: preserve original field definitions verbatim in extracted
    module and validate via split guardrail tests.
- Risk: full crate tests are expensive and may stall in this environment.
  - Mitigation: run fast guardrail/regression suites and document compile-test
    attempts separately.

## Interfaces and Contracts

- `Cli` remains the public parser type; extracted fields are provided through
  `CliExecutionDomainFlags` with clap flatten.
- Guardrail contract:
  `scripts/dev/test-cli-args-domain-split.sh` enforces `<3000` and required
  module markers.

## ADR References

- Not required.
