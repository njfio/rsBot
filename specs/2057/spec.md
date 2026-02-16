# Spec #2057

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2057

## Problem Statement

Roadmap status reporting currently updates inline markdown blocks but does not
publish a stable machine-readable artifact with a documented schema. This makes
auditing and downstream automation brittle.

## Acceptance Criteria

- AC-1: A versioned roadmap status artifact schema is published in-repo and
  documents required JSON fields.
- AC-2: A generator command emits deterministic JSON + Markdown artifacts when
  run with fixture input and a fixed timestamp.
- AC-3: The generator fails closed with clear errors for malformed config,
  malformed fixture payloads, or invalid timestamp input.

## Scope

In:

- Define roadmap status artifact JSON schema.
- Implement deterministic artifact generator script.
- Add generator tests for happy path and fail-closed regressions.

Out:

- Auto-committing generated artifacts to repository history.
- Replacing existing roadmap status sync block generator.

## Conformance Cases

- C-01 (AC-1, unit): schema file exists, is versioned, and includes required
  properties for summary/group/gap sections.
- C-02 (AC-2, functional): generator run with fixture + fixed timestamp
  produces JSON/Markdown artifacts with stable hashes across repeated runs.
- C-03 (AC-3, regression): malformed fixture/config and invalid timestamp each
  return non-zero and emit deterministic error text.

## Success Metrics

- Operators can generate identical artifacts locally and in CI for identical
  inputs.
- Artifact schema can be referenced by docs/workflows as the single contract.
