# Plan #2227

Status: Implemented
Spec: specs/2227/spec.md

## Approach

1. RED: add failing unit test asserting Google tool-schema output excludes `additionalProperties`.
2. GREEN: implement recursive schema sanitizer in Google adapter and apply during function-declaration conversion.
3. VERIFY: run `tau-ai` tests and live Gemini command with real key.

## Affected Modules

- `crates/tau-ai/src/google.rs`
- `specs/milestones/m43/index.md`
- `specs/2227/spec.md`
- `specs/2227/plan.md`
- `specs/2227/tasks.md`

## Risks and Mitigations

- Risk: removing unsupported keys might drop required schema semantics.
  - Mitigation: preserve all non-unsupported keys; add regression tests for nested properties.
- Risk: Google API errors persist for other unsupported keywords.
  - Mitigation: inspect live error payload and extend sanitizer minimally if needed.

## Interfaces and Contracts

- RED/Green tests:
  - `cargo test -p tau-ai --lib google::tests::conformance_google_function_declaration_strips_additional_properties -- --exact`
- Verification:
  - `cargo test -p tau-ai --lib`
  - `cargo fmt --check`
  - live command using real Gemini key

## ADR References

- Not required.
