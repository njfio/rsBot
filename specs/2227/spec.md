# Spec #2227

Status: Implemented
Milestone: specs/milestones/m43/index.md
Issue: https://github.com/njfio/Tau/issues/2227

## Problem Statement

Live Gemini calls through `tau-coding-agent` currently fail with HTTP 400
because Gemini rejects `additionalProperties` inside tool function declaration
schemas emitted by `tau-ai` Google adapter.

## Acceptance Criteria

- AC-1: Google adapter sanitizes tool parameter schemas before request dispatch,
  removing unsupported `additionalProperties` keys recursively.
- AC-2: Unit/conformance tests prove sanitized output omits
  `additionalProperties` while preserving supported schema structure.
- AC-3: Live Gemini prompt run succeeds using real provider credentials.
- AC-4: Existing touched crate test suites pass.

## Scope

In:

- `crates/tau-ai/src/google.rs`
- tests in `crates/tau-ai/src/google.rs`
- provider smoke validation evidence/log update

Out:

- cross-provider schema redesign
- tool contract JSON schema rewrites

## Conformance Cases

- C-01 (AC-1, unit): `build_generate_content_body` output for Google tool declarations has no `additionalProperties` keys.
- C-02 (AC-2, conformance): nested schema objects remain present (type/properties/required) after sanitization.
- C-03 (AC-3, functional): `tau-coding-agent` prompt run with `google/gemini-2.5-pro` returns success against real key.
- C-04 (AC-4, regression): `cargo test -p tau-ai --lib` and `cargo fmt --check` pass.

## Success Metrics

- Gemini live-provider run passes where previous run failed with `additionalProperties` API validation errors.
