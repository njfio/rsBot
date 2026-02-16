# Milestone M45: Advanced Live Validation Expansion

Status: Implemented

## Objective

Expand Tau's repeatable validation to cover advanced end-to-end runtime
capabilities across providers, including Codex direct runs, additional
OpenRouter models, long-output stress, streaming mode, retry/failure behavior,
session continuity, and multi-tool execution behavior.

## Scope

In scope:

- Live capability test coverage for items 1-7 requested by product
- Repeatable harness/script updates with deterministic local test verification
- Validation runs and pass/fail summaries outside version control (`.tau/`)

Out of scope:

- New provider adapters or protocol redesign
- Dashboard/voice/browser feature implementation (tracked separately)
- CI policy changes for required checks

## Success Signals

- Each item 1-7 has a named, executable test path.
- All 1-7 tests pass in local validation with provided provider credentials.
- Harness output remains out of git-tracked repository files.

## Issue Hierarchy

Milestone: GitHub milestone `M45 Live Validation Expansion`

Epic:

- `#2240` Epic: M45 advanced live validation expansion

Story:

- `#2241` Story: M45.1 implement and validate 1-7 advanced runtime tests

Task:

- `#2242` Task: M45.1.1 add harness coverage and deterministic tests

Subtask:

- `#2243` Subtask: M45.1.1a execute live matrix and prove pass state
