# Plan #2040

Status: Implemented
Spec: specs/2040/spec.md

## Approach

Use a two-step execution model:

1. Planning artifact:
   `#2058` publishes split-map schema/generator/reports and ownership boundaries.
2. Implementation split:
   `#2059` extracts high-volume flag domains into dedicated module(s), updates
   guardrails, and captures parity evidence.

## Affected Modules

- `crates/tau-cli/src/cli_args.rs`
- `crates/tau-cli/src/cli_args/*`
- `scripts/dev/test-cli-args-domain-split.sh`
- `scripts/dev/cli-args-split-map.sh`
- `tasks/reports/m25-cli-args-split-map.json`
- `tasks/reports/m25-cli-args-split-map.md`

## Risks and Mitigations

- Risk: large clap refactor introduces silent CLI argument regressions.
  - Mitigation: preserve original field attributes and enforce split marker
    tests plus crate-scoped verification.
- Risk: compile/test runtime cost slows feedback loops.
  - Mitigation: rely on fast guardrail scripts and scoped checks first, then
    expand to broader suites as capacity permits.

## Interfaces and Contracts

- `Cli` remains the public parse surface for callers.
- `scripts/dev/test-cli-args-domain-split.sh` is the threshold and marker
  contract for this task.

## ADR References

- Not required.
