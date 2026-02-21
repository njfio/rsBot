# Spec: Issue #3008 - Add tau-diagnostics boundary conformance tests

Status: Implemented

## Problem Statement
`tau-diagnostics` currently has limited direct crate-local tests. Key parser and audit-file boundary behaviors are only partially covered, leaving fail-closed guarantees under-tested.

## Acceptance Criteria

### AC-1 Doctor command parser fail-closed boundaries are covered
Given `parse_doctor_command_args`,
When duplicate `--online` flags or unknown flags are provided,
Then parsing fails closed with `DOCTOR_USAGE`.

### AC-2 Audit summarization handles blank-line and mixed-record boundaries
Given `summarize_audit_file`,
When JSONL fixtures include blank lines plus mixed prompt/tool/lifecycle records,
Then record counters and aggregate fields remain deterministic and correct.

### AC-3 Malformed audit JSON reports line-context failure
Given `summarize_audit_file`,
When a malformed JSON line appears in input,
Then the returned error includes line-location context and fails closed.

### AC-4 Targeted tau-diagnostics tests pass
Given the crate test suite,
When running targeted and crate-level tests,
Then all new and existing tests pass.

## Scope

### In Scope
- Test additions in `crates/tau-diagnostics/src/lib.rs`.
- M180/#3008 spec artifacts.
- Targeted validation commands for `tau-diagnostics`.

### Out of Scope
- Runtime behavior changes unrelated to tests.
- Cross-crate refactors.
- New dependency introduction.

## Conformance Cases
- C-01: duplicate/unknown doctor command flags fail closed.
- C-02: mixed-record + blank-line audit fixture summarizes with expected counts.
- C-03: malformed JSON audit fixture returns contextual parse/read failure.
- C-04: `cargo test -p tau-diagnostics` passes.

## Success Metrics / Observable Signals
- `cargo test -p tau-diagnostics unit_spec_3008_c01_diagnostics_doctor_arg_parser_rejects_duplicate_online -- --exact`
- `cargo test -p tau-diagnostics functional_spec_3008_c02_summarize_audit_file_handles_blank_lines_and_mixed_records -- --exact`
- `cargo test -p tau-diagnostics regression_spec_3008_c03_summarize_audit_file_reports_line_context_for_malformed_json -- --exact`
- `cargo test -p tau-diagnostics`

## Approval Gate
P2 scope: agent-authored spec, self-reviewed, implementation proceeds with human review in PR.
