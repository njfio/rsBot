# Plan #2065

Status: Implemented
Spec: specs/2065/spec.md

## Approach

1. Add failing guardrail checks for `github_issues_runtime.rs` threshold and
   module markers.
2. Extract high-volume runtime domains into
   `crates/tau-github-issues-runtime/src/github_issues_runtime/` modules while
   preserving public entrypoints.
3. Keep behavior stable by minimizing API signature changes and retaining
   reason-code/error-envelope contracts.
4. Run targeted validation commands and capture closure evidence.

## Affected Modules

- `crates/tau-github-issues-runtime/src/github_issues_runtime.rs`
- `crates/tau-github-issues-runtime/src/github_issues_runtime/*`
- split guardrail scripts under `scripts/dev/`

## Risks and Mitigations

- Risk: extraction changes bridge error or retry semantics.
  - Mitigation: preserve reason-code builders and validate contract outputs.
- Risk: cross-crate compile/test evidence blocked by unrelated branch drift.
  - Mitigation: anchor proof to split guardrails plus integration contract
    suites.

## Interfaces and Contracts

- Keep runtime public entrypoint signatures stable for consuming crates.
- Preserve ingest/sync payload schema and reason-code envelopes.

## ADR References

- Not required.
