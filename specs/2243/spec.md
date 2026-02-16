# Spec #2243

Status: Implemented
Milestone: specs/milestones/m45/index.md
Issue: https://github.com/njfio/Tau/issues/2243

## Problem Statement

Tau needs repeatable, executable validation coverage for advanced runtime
capabilities requested by product. Current live harness coverage is partial and
does not explicitly gate all requested capabilities (1-7).

## Acceptance Criteria

- AC-1: Given OpenAI Codex direct model execution, when a live harness case is
  run, then the run completes with at least one tool call and expected artifacts.
- AC-2: Given OpenRouter Kimi and Minimax models, when dedicated harness cases
  run, then both cases complete and produce expected artifacts.
- AC-3: Given a long-output stress scenario, when the harness case runs, then
  the output artifact meets a minimum content-length threshold.
- AC-4: Given streaming mode validation across providers, when stream-specific
  cases run, then runs complete and produce expected artifacts.
- AC-5: Given retry/failure behavior requirements, when retry tests execute,
  then timeout, 429 retry/backoff, and retry-budget behavior are validated.
- AC-6: Given a persisted session path, when a resume workflow runs in two
  phases, then artifacts from both phases exist and the same session store is reused.
- AC-7: Given multi-tool execution requirements, when a dedicated case runs,
  then multiple tool executions occur and expected multi-file artifacts are created.
- AC-8: Given deterministic local harness testing, when script-level tests run,
  then new/updated harness behavior for added cases passes.

## Scope

In scope:

- Extend `scripts/dev/live-capability-matrix.sh` with explicit cases for AC-1,
  AC-2, AC-3, AC-4, AC-6, AC-7.
- Extend deterministic script test coverage in
  `scripts/dev/test-live-capability-matrix.sh`.
- Add a repeatable validation runner script that executes AC-1..AC-8 checks and
  reports pass/fail summaries under `.tau/`.
- Execute validations locally and confirm all requested items pass.

Out of scope:

- New provider integrations or SDK changes
- CI policy changes
- Dashboard/voice/browser feature implementation

## Conformance Cases

- C-01 (AC-1, Integration): `research_openai_codex` live case passes completion,
  tool-call presence, and artifact checks.
- C-02 (AC-2, Integration): `research_openrouter_kimi` and
  `blog_openrouter_minimax` live cases pass completion and artifact checks.
- C-03 (AC-3, Integration): `long_output_openai_codex` live case creates
  `long_output.md` meeting minimum word-count threshold.
- C-04 (AC-4, Integration): streaming-enabled provider cases pass with expected
  artifacts.
- C-05 (AC-5, Functional/Regression): `tau-ai` retry/timeout integration tests
  for 429 backoff, timeout, and retry budget pass.
- C-06 (AC-6, Integration): two-phase session continuity case passes with
  persisted session reuse and phase artifacts.
- C-07 (AC-7, Integration): parallel/multi-tool case records >= 2 tool calls and
  required artifacts.
- C-08 (AC-8, Functional): deterministic harness test script passes with new case
  coverage.

## Success Metrics

- A single command executes all 1-7 validations and prints an overall PASS.
- All outputs are written under `.tau/reports/live-validation/` (untracked).
- Validation proof run:
  `scripts/dev/validate-advanced-capabilities.sh --run-id 2243-advanced-v4 --max-turns 8 --timeout-ms 120000`
  completed with PASS.
