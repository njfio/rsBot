# Spec #2347

Status: Accepted
Milestone: specs/milestones/m56/index.md
Issue: https://github.com/njfio/Tau/issues/2347

## Problem Statement

`tasks/resolution-roadmap.md` still marks dynamic model catalog discovery and
remote-source merge policy as pending, despite existing `tau-provider`
integration paths that parse remote payloads, merge overlays with built-ins,
and fall back to cache when remote refresh fails.

## Scope

In scope:

- Validate dynamic catalog discovery/merge behavior with executable tests.
- Add a single verifier script for repeatable local validation.
- Update roadmap claim line with resolved status and command evidence.

Out of scope:

- New provider transports or broader catalog redesign.
- Expanding roadmap closure to unrelated unchecked checklist items.

## Acceptance Criteria

- AC-1: Given the `tau-provider` model-catalog implementation, when running the
  mapped parsing/refresh/merge tests, then remote discovery and merge semantics
  pass without regression.
- AC-2: Given remote unavailability or invalid cache states, when running mapped
  fallback tests, then catalog loading degrades safely to cache/built-in with
  explicit source reporting.
- AC-3: Given roadmap review, when inspecting the top-level pending claim for
  dynamic catalog discovery, then it is marked resolved with executable command
  evidence.

## Conformance Cases

- C-01 (AC-1, conformance): `spec_c01_parse_model_catalog_payload_accepts_openrouter_models_shape`
  passes.
- C-02 (AC-1, integration): `integration_model_catalog_remote_refresh_writes_cache_and_supports_offline_reuse`
  passes.
- C-03 (AC-1, integration): `integration_spec_c02_remote_refresh_merges_openrouter_entries_with_builtin_catalog`
  passes.
- C-04 (AC-2, regression): `regression_model_catalog_remote_failure_falls_back_to_cache`
  passes.
- C-05 (AC-3, conformance): roadmap line is updated to resolved with verifier
  command(s).
