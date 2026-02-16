# Milestone M43: Gemini Live Provider Compatibility Fix

Status: Draft

## Objective

Restore live Gemini provider execution by making Google tool-function schema
serialization compatible with Gemini API validation requirements.

## Scope

In scope:

- provider-specific schema sanitization in `tau-ai` Google adapter
- unit/conformance tests for sanitized schema output
- live provider validation run for Gemini with real credentials

Out of scope:

- generic cross-provider schema normalization redesign
- non-Google provider runtime changes

## Success Signals

- M43 hierarchy exists and is active with epic/story/task/subtask linkage.
- Gemini live prompt run succeeds via `tau-coding-agent` with real API key.
- Existing `tau-ai`/provider tests remain green.

## Issue Hierarchy

Milestone: GitHub milestone `M43 Gemini Live Provider Compatibility Fix`

Epic:

- `#2224` Epic: M43 Gemini live-provider compatibility fix

Story:

- `#2225` Story: M43.1 Sanitize Google tool schemas for Gemini compatibility

Task:

- `#2226` Task: M43.1.1 Google schema sanitization and live Gemini validation

Subtask:

- `#2227` Subtask: M43.1.1a Fix Gemini tool-schema compatibility and verify live run
